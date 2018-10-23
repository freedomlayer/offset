use std::marker::Unpin;
use futures::{future, stream, Stream, StreamExt, Sink, SinkExt};
use futures::channel::mpsc;
use proto::relay::messages::TunnelMessage;
use timer::TimerTick;

#[derive(Debug)]
pub enum ClientTunnelError {
    TimerClosed,
    RemoteTimeout,
    SendToUserError,
    SendToTunnelError,
}

#[derive(Debug, Clone)]
enum ClientTunnelEvent {
    TimerTick,
    TimerClosed,
    TunnelChannelClosed,
    UserChannelClosed,
    MessageFromTunnel(TunnelMessage),
    MessageFromUser(Vec<u8>),
}

/// Run the keepalive maintenance, exposing to the user the ability to send and receive Vec<u8>
/// frames.
pub async fn client_tunnel<TTS,TTSE,FTR,UFTS,UFTSE,UTTR,TS>(to_tunnel_sender: TTS, from_tunnel_receiver: FTR, 
                           user_from_tunnel_sender: UFTS, user_to_tunnel_receiver: UTTR,
                           timer_stream: TS,
                           keepalive_ticks: usize) -> Result<(), ClientTunnelError> 
where
    TTS: Sink<SinkItem=TunnelMessage, SinkError=TTSE> + Unpin,
    FTR: Stream<Item=TunnelMessage> + Unpin,
    UFTS: Sink<SinkItem=Vec<u8>, SinkError=UFTSE> + Unpin,
    UTTR: Stream<Item=Vec<u8>> + Unpin,
    TS: Stream<Item=TimerTick> + Unpin,
{
    await!(inner_client_tunnel(to_tunnel_sender, from_tunnel_receiver,
                        user_from_tunnel_sender, user_to_tunnel_receiver,
                        timer_stream,
                        keepalive_ticks,
                        None))
}


async fn inner_client_tunnel<TTS,TTSE,FTR,UFTS,UFTSE,UTTR,TS>(mut to_tunnel_sender: TTS, from_tunnel_receiver: FTR, 
                           mut user_from_tunnel_sender: UFTS, user_to_tunnel_receiver: UTTR,
                           timer_stream: TS,
                           keepalive_ticks: usize,
                           mut opt_event_sender: Option<mpsc::Sender<ClientTunnelEvent>>) -> Result<(), ClientTunnelError> 
where
    TTS: Sink<SinkItem=TunnelMessage, SinkError=TTSE> + Unpin,
    FTR: Stream<Item=TunnelMessage> + Unpin,
    UFTS: Sink<SinkItem=Vec<u8>, SinkError=UFTSE> + Unpin,
    UTTR: Stream<Item=Vec<u8>> + Unpin,
    TS: Stream<Item=TimerTick> + Unpin,
{
    let timer_stream = timer_stream
        .map(|_| ClientTunnelEvent::TimerTick)
        .chain(stream::once(future::ready(ClientTunnelEvent::TimerClosed)));

    let from_tunnel_receiver = from_tunnel_receiver
        .map(|tunnel_message| ClientTunnelEvent::MessageFromTunnel(tunnel_message))
        .chain(stream::once(future::ready(ClientTunnelEvent::TunnelChannelClosed)));

    let user_to_tunnel_receiver = user_to_tunnel_receiver
        .map(|vec| ClientTunnelEvent::MessageFromUser(vec))
        .chain(stream::once(future::ready(ClientTunnelEvent::UserChannelClosed)));

    let mut events = timer_stream
        .select(from_tunnel_receiver)
        .select(user_to_tunnel_receiver);

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
            ClientTunnelEvent::MessageFromTunnel(tunnel_message) => {
                ticks_to_close = keepalive_ticks;
                if let TunnelMessage::Message(vec) = tunnel_message {
                    await!(user_from_tunnel_sender.send(vec))
                        .map_err(|_| ClientTunnelError::SendToUserError)?;
                }
            },
            ClientTunnelEvent::MessageFromUser(vec) => {
                let tunnel_message = TunnelMessage::Message(vec);
                await!(to_tunnel_sender.send(tunnel_message))
                    .map_err(|_| ClientTunnelError::SendToTunnelError)?;
                ticks_to_send_keepalive = keepalive_ticks / 2;
            },
            ClientTunnelEvent::TimerTick => {
                ticks_to_close = ticks_to_close.saturating_sub(1);
                ticks_to_send_keepalive = ticks_to_send_keepalive.saturating_sub(1);
                if ticks_to_close == 0 {
                    return Err(ClientTunnelError::RemoteTimeout);
                }
                if ticks_to_send_keepalive == 0 {
                    let tunnel_message = TunnelMessage::KeepAlive;
                    await!(to_tunnel_sender.send(tunnel_message))
                        .map_err(|_| ClientTunnelError::SendToTunnelError)?;
                    ticks_to_send_keepalive = keepalive_ticks / 2;
                }
            },
            ClientTunnelEvent::TimerClosed => return Err(ClientTunnelError::TimerClosed),
            ClientTunnelEvent::TunnelChannelClosed |
            ClientTunnelEvent::UserChannelClosed => break,
        }
    }
    Ok(())
}


#[cfg(test)]
mod tests {
    use super::*;
    use futures::{FutureExt, TryFutureExt};
    use futures::executor::ThreadPool;
    use futures::task::{Spawn, SpawnExt};
    use timer::{create_timer_incoming};


    async fn task_client_tunnel_basic(mut spawner: impl Spawn + Clone) -> Result<(),()> {
        // Create a mock time service:
        let (mut tick_sender, tick_receiver) = mpsc::channel::<()>(0);
        let mut timer_client = create_timer_incoming(tick_receiver, spawner.clone()).unwrap();

        let (event_sender, mut event_receiver) = mpsc::channel(0);

        let (local_sender, mut remote_receiver) = mpsc::channel::<TunnelMessage>(0);
        let (mut remote_sender, local_receiver) = mpsc::channel::<TunnelMessage>(0);

        let (user_from_tunnel_sender, mut user_from_tunnel_receiver) = mpsc::channel::<Vec<u8>>(0);
        let (mut user_to_tunnel_sender, user_to_tunnel_receiver) = mpsc::channel::<Vec<u8>>(0);

        let timer_stream = await!(timer_client.request_timer_stream()).unwrap();
        let keepalive_ticks = 16;
        let fut_client_tunnel = inner_client_tunnel(local_sender, local_receiver, 
                           user_from_tunnel_sender, user_to_tunnel_receiver,
                           timer_stream,
                           keepalive_ticks,
                           Some(event_sender))
            .map_err(|e| println!("client_tunnel error: {:?}", e))
            .map(|_| ());

        spawner.spawn(fut_client_tunnel).unwrap();


        // Send from user to remote:
        await!(user_to_tunnel_sender.send(vec![1,2,3])).unwrap();
        await!(event_receiver.next()).unwrap();
        match await!(remote_receiver.next()).unwrap() {
            TunnelMessage::Message(vec) => assert_eq!(vec, vec![1,2,3]),
            _ => unreachable!(),
        };

        // User can not see Keepalive messages sent from remote:
        await!(remote_sender.send(TunnelMessage::KeepAlive)).unwrap();
        await!(event_receiver.next()).unwrap();

        // Send from remote to user:
        await!(remote_sender.send(TunnelMessage::Message(vec![3,2,1]))).unwrap();
        await!(event_receiver.next()).unwrap();
        let vec = await!(user_from_tunnel_receiver.next()).unwrap();
        assert_eq!(vec, vec![3,2,1]);        

        // Move time forward
        for _ in 0 .. 8 {
            await!(tick_sender.send(())).unwrap();
            await!(event_receiver.next()).unwrap();
        }

        // We expect to see a keepalive being sent:
        match await!(remote_receiver.next()).unwrap() {
            TunnelMessage::KeepAlive => {},
            _ => unreachable!(),
        };

        // Remote sends a keepalive:
        await!(remote_sender.send(TunnelMessage::KeepAlive)).unwrap();
        await!(event_receiver.next()).unwrap();

        // Move time forward
        for _ in 0 .. 16 {
            await!(tick_sender.send(())).unwrap();
            await!(event_receiver.next()).unwrap();
        }

        // Tunnel should be closed, 
        // because remote haven't sent a keepalive for a long time:
        let res = await!(user_from_tunnel_receiver.next());
        assert!(res.is_none());

        Ok(())
    }

    #[test]
    fn test_client_tunnel_basic() {
        let mut thread_pool = ThreadPool::new().unwrap();
        thread_pool.run(task_client_tunnel_basic(thread_pool.clone())).unwrap();
    }
}
