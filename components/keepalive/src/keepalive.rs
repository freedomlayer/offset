use futures::channel::mpsc;
use futures::task::{Spawn, SpawnExt};
use futures::{future, stream, FutureExt, Sink, SinkExt, Stream, StreamExt, TryFutureExt};
use std::marker::Unpin;
use timer::{TimerClient, TimerTick};

use common::conn::{BoxFuture, ConnPair, FutTransform};
use common::select_streams::{select_streams, BoxStream};

use proto::keepalive::messages::KaMessage;
use proto::keepalive::serialize::{deserialize_ka_message, serialize_ka_message};

#[derive(Debug)]
pub enum KeepAliveError {
    // TimerClosed,
    RemoteTimeout,
    DeserializeError,
}

#[derive(Debug, Clone)]
enum KeepAliveEvent {
    TimerTick,
    TimerClosed,
    RemoteChannelClosed,
    UserChannelClosed,
    MessageFromRemote(Vec<u8>),
    MessageFromUser(Vec<u8>),
}

/*
/// Run the keepalive maintenance, exposing to the user the ability to send and receive Vec<u8>
/// frames.
pub async fn keepalive_loop<TR,FR,TU,FU,TS>(to_remote: TR, from_remote: FR,
                           to_user: TU, from_user: FU,
                           timer_stream: TS,
                           keepalive_ticks: usize) -> Result<(), KeepAliveError>
where
    TR: Sink<SinkItem=Vec<u8>> + Unpin,
    FR: Stream<Item=Vec<u8>> + Unpin,
    TU: Sink<SinkItem=Vec<u8>> + Unpin,
    FU: Stream<Item=Vec<u8>> + Unpin,
    TS: Stream<Item=TimerTick> + Unpin,
{
    await!(inner_keepalive_loop(to_remote, from_remote,
                        to_user, from_user,
                        timer_stream,
                        keepalive_ticks,
                        None))
}
*/

async fn inner_keepalive_loop<TR, FR, TU, FU, TS>(
    mut to_remote: TR,
    from_remote: FR,
    mut to_user: TU,
    from_user: FU,
    timer_stream: TS,
    keepalive_ticks: usize,
    mut opt_event_sender: Option<mpsc::Sender<KeepAliveEvent>>,
) -> Result<(), KeepAliveError>
where
    TR: Sink<Vec<u8>> + Unpin,
    FR: Stream<Item = Vec<u8>> + Unpin + Send,
    TU: Sink<Vec<u8>> + Unpin,
    FU: Stream<Item = Vec<u8>> + Unpin + Send,
    TS: Stream<Item = TimerTick> + Unpin + Send,
{
    let timer_stream = timer_stream
        .map(|_| KeepAliveEvent::TimerTick)
        .chain(stream::once(future::ready(KeepAliveEvent::TimerClosed)));

    let from_remote = from_remote
        .map(KeepAliveEvent::MessageFromRemote)
        .chain(stream::once(future::ready(
            KeepAliveEvent::RemoteChannelClosed,
        )));

    let from_user = from_user
        .map(KeepAliveEvent::MessageFromUser)
        .chain(stream::once(future::ready(
            KeepAliveEvent::UserChannelClosed,
        )));

    let mut events = select_streams![timer_stream, from_remote, from_user];

    // Amount of ticks remaining until we decide to close this connection (Because remote is idle):
    let mut ticks_to_close = keepalive_ticks;
    // Amount of ticks remaining until we need to send a new keepalive (To make sure remote side
    // knows we are alive).
    let mut ticks_to_send_keepalive = keepalive_ticks / 2;

    while let Some(event) = await!(events.next()) {
        if let Some(ref mut event_sender) = opt_event_sender {
            let _ = await!(event_sender.send(event.clone()));
        }
        match event {
            KeepAliveEvent::MessageFromRemote(ser_ka_message) => {
                let ka_message = deserialize_ka_message(&ser_ka_message)
                    .map_err(|_| KeepAliveError::DeserializeError)?;
                ticks_to_close = keepalive_ticks;
                if let KaMessage::Message(message) = ka_message {
                    if await!(to_user.send(message)).is_err() {
                        warn!("keepalive_loop(): Can not send to local side");
                        break;
                    }
                }
            }
            KeepAliveEvent::MessageFromUser(message) => {
                let ka_message = KaMessage::Message(message);
                let ser_ka_message = serialize_ka_message(&ka_message);
                if await!(to_remote.send(ser_ka_message)).is_err() {
                    warn!("keepalive_loop(): Can not send to remote side");
                    break;
                }
                ticks_to_send_keepalive = keepalive_ticks / 2;
            }
            KeepAliveEvent::TimerTick => {
                ticks_to_close = ticks_to_close.saturating_sub(1);
                ticks_to_send_keepalive = ticks_to_send_keepalive.saturating_sub(1);
                if ticks_to_close == 0 {
                    return Err(KeepAliveError::RemoteTimeout);
                }
                if ticks_to_send_keepalive == 0 {
                    let ka_message = KaMessage::KeepAlive;
                    let ser_ka_message = serialize_ka_message(&ka_message);
                    if await!(to_remote.send(ser_ka_message)).is_err() {
                        warn!("Keepalive_loop(): Can not send to remote side");
                        break;
                    }
                    ticks_to_send_keepalive = keepalive_ticks / 2;
                }
            }
            KeepAliveEvent::TimerClosed
            | KeepAliveEvent::RemoteChannelClosed
            | KeepAliveEvent::UserChannelClosed => break,
        }
    }
    Ok(())
}

#[derive(Clone)]
pub struct KeepAliveChannel<S> {
    timer_client: TimerClient,
    keepalive_ticks: usize,
    spawner: S,
}

impl<S> KeepAliveChannel<S>
where
    S: Spawn + Send,
{
    pub fn new(
        timer_client: TimerClient,
        keepalive_ticks: usize,
        spawner: S,
    ) -> KeepAliveChannel<S> {
        KeepAliveChannel {
            timer_client,
            keepalive_ticks,
            spawner,
        }
    }

    /// Transform a usual `Vec<u8>` connection end into a connection end that performs
    /// keepalives automatically. The output `conn_pair` looks exactly like the input pair, however
    /// it also maintains keepalives.
    fn transform_keepalive(
        &mut self,
        conn_pair: ConnPair<Vec<u8>, Vec<u8>>,
    ) -> BoxFuture<'_, ConnPair<Vec<u8>, Vec<u8>>> {
        let (to_remote, from_remote) = conn_pair;

        let (to_user, user_receiver) = mpsc::channel::<Vec<u8>>(0);
        let (user_sender, from_user) = mpsc::channel::<Vec<u8>>(0);

        Box::pin(
            async move {
                if let Ok(timer_stream) = await!(self.timer_client.request_timer_stream()) {
                    let keepalive_fut = inner_keepalive_loop(
                        to_remote,
                        from_remote,
                        to_user,
                        from_user,
                        timer_stream,
                        self.keepalive_ticks,
                        None,
                    )
                    .map_err(|e| {
                        warn!(
                            "transform_keepalive(): inner_keepalive_loop() error: {:?}",
                            e
                        )
                    })
                    .then(|_| future::ready(()));

                    self.spawner.spawn(keepalive_fut).unwrap();
                } else {
                    // Note: In this case the user will notice there is an error when he tries to
                    // use the connection, because to_user, from_user are dropped
                    warn!("transform_keepalive(): Error requesting timer stream");
                }

                (user_sender, user_receiver)
            },
        )
    }
}

impl<S> FutTransform for KeepAliveChannel<S>
where
    S: Spawn + Send,
{
    type Input = ConnPair<Vec<u8>, Vec<u8>>;
    type Output = ConnPair<Vec<u8>, Vec<u8>>;

    fn transform(&mut self, input: Self::Input) -> BoxFuture<'_, Self::Output> {
        self.transform_keepalive(input)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::executor::ThreadPool;
    use futures::task::{Spawn, SpawnExt};
    use futures::FutureExt;
    use timer::create_timer_incoming;

    /// Util function for tests
    /// Possibly remove it in the future and test the KeepAliveChannel interface directly.
    fn keepalive_channel<TR, FR, TS, S>(
        to_remote: TR,
        from_remote: FR,
        timer_stream: TS,
        keepalive_ticks: usize,
        mut spawner: S,
    ) -> (mpsc::Sender<Vec<u8>>, mpsc::Receiver<Vec<u8>>)
    where
        TR: Sink<Vec<u8>> + Unpin + Send + 'static,
        FR: Stream<Item = Vec<u8>> + Unpin + Send + 'static,
        TS: Stream<Item = TimerTick> + Unpin + Send + 'static,
        S: Spawn,
    {
        let (to_user, user_receiver) = mpsc::channel::<Vec<u8>>(0);
        let (user_sender, from_user) = mpsc::channel::<Vec<u8>>(0);

        let keepalive_fut = inner_keepalive_loop(
            to_remote,
            from_remote,
            to_user,
            from_user,
            timer_stream,
            keepalive_ticks,
            None,
        )
        .map_err(|e| error!("[KeepAlive] inner_keepalive_loop() error: {:?}", e))
        .then(|_| future::ready(()));

        spawner.spawn(keepalive_fut).unwrap();

        (user_sender, user_receiver)
    }

    async fn task_keepalive_loop_basic(mut spawner: impl Spawn + Clone) {
        // Create a mock time service:
        let (mut tick_sender, tick_receiver) = mpsc::channel::<()>(0);
        let mut timer_client = create_timer_incoming(tick_receiver, spawner.clone()).unwrap();

        let (event_sender, mut event_receiver) = mpsc::channel(0);

        let (to_remote, mut remote_receiver) = mpsc::channel::<Vec<u8>>(0);
        let (mut remote_sender, from_remote) = mpsc::channel::<Vec<u8>>(0);

        let (to_user, mut user_receiver) = mpsc::channel::<Vec<u8>>(0);
        let (mut user_sender, from_user) = mpsc::channel::<Vec<u8>>(0);

        let timer_stream = await!(timer_client.request_timer_stream()).unwrap();
        let keepalive_ticks = 16;
        let fut_keepalive_loop = inner_keepalive_loop(
            to_remote,
            from_remote,
            to_user,
            from_user,
            timer_stream,
            keepalive_ticks,
            Some(event_sender),
        )
        // .map_err(|e| println!("client_tunnel error: {:?}", e))
        .map(|_| ());

        spawner.spawn(fut_keepalive_loop).unwrap();

        // Send from user to remote:
        await!(user_sender.send(vec![1, 2, 3])).unwrap();
        await!(event_receiver.next()).unwrap();
        let vec = await!(remote_receiver.next()).unwrap();
        assert_eq!(
            vec,
            serialize_ka_message(&KaMessage::Message(vec![1, 2, 3]))
        );

        // User can not see Keepalive messages sent from remote:
        let vec = serialize_ka_message(&KaMessage::KeepAlive);
        await!(remote_sender.send(vec)).unwrap();
        await!(event_receiver.next()).unwrap();

        // Send from remote to user:
        let vec = serialize_ka_message(&KaMessage::Message(vec![3, 2, 1]));
        await!(remote_sender.send(vec)).unwrap();
        await!(event_receiver.next()).unwrap();
        let vec = await!(user_receiver.next()).unwrap();
        assert_eq!(vec, vec![3, 2, 1]);

        // Move time forward
        for _ in 0..8usize {
            await!(tick_sender.send(())).unwrap();
            await!(event_receiver.next()).unwrap();
        }

        // We expect to see a keepalive being sent:
        let vec = await!(remote_receiver.next()).unwrap();
        assert_eq!(vec, serialize_ka_message(&KaMessage::KeepAlive));

        // Remote sends a keepalive:
        let vec = serialize_ka_message(&KaMessage::KeepAlive);
        await!(remote_sender.send(vec)).unwrap();
        await!(event_receiver.next()).unwrap();

        // Move time forward
        for _ in 0..16usize {
            await!(tick_sender.send(())).unwrap();
            await!(event_receiver.next()).unwrap();
        }

        // Channel should be closed,
        // because remote haven't sent a keepalive for a long time:
        let res = await!(user_receiver.next());
        assert!(res.is_none());
    }

    #[test]
    fn test_keepalive_loop_basic() {
        let mut thread_pool = ThreadPool::new().unwrap();
        thread_pool.run(task_keepalive_loop_basic(thread_pool.clone()));
    }

    async fn task_keepalive_channel_basic(spawner: impl Spawn + Clone) {
        // Create a mock time service:
        let (mut tick_sender, tick_receiver) = mpsc::channel::<()>(0);
        let mut timer_client = create_timer_incoming(tick_receiver, spawner.clone()).unwrap();

        let keepalive_ticks = 16;

        /*       A     B
         *   --> | --> | -->
         *       |     |
         *   <-- | <-- | <--
         */

        let (a_sender, b_receiver) = mpsc::channel(0);
        let (b_sender, a_receiver) = mpsc::channel(0);

        let timer_stream = await!(timer_client.request_timer_stream()).unwrap();
        let (mut a_sender, mut a_receiver) = keepalive_channel(
            a_sender,
            a_receiver,
            timer_stream,
            keepalive_ticks,
            spawner.clone(),
        );

        let timer_stream = await!(timer_client.request_timer_stream()).unwrap();
        let (mut b_sender, mut b_receiver) = keepalive_channel(
            b_sender,
            b_receiver,
            timer_stream,
            keepalive_ticks,
            spawner.clone(),
        );

        await!(a_sender.send(vec![1, 2, 3])).unwrap();
        assert_eq!(await!(b_receiver.next()).unwrap(), vec![1, 2, 3]);

        await!(b_sender.send(vec![3, 2, 1])).unwrap();
        assert_eq!(await!(a_receiver.next()).unwrap(), vec![3, 2, 1]);

        // Move some time forward
        for _ in 0..(keepalive_ticks / 2) + 1 {
            await!(tick_sender.send(())).unwrap();
        }

        await!(a_sender.send(vec![1, 2, 3])).unwrap();
        assert_eq!(await!(b_receiver.next()).unwrap(), vec![1, 2, 3]);

        await!(b_sender.send(vec![3, 2, 1])).unwrap();
        assert_eq!(await!(a_receiver.next()).unwrap(), vec![3, 2, 1]);
    }

    #[test]
    fn test_keepalive_channel_basic() {
        let mut thread_pool = ThreadPool::new().unwrap();
        thread_pool.run(task_keepalive_channel_basic(thread_pool.clone()));
    }
}
