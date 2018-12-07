use std::marker::Unpin;
use futures::{select, future, FutureExt, TryFutureExt, stream, Stream, StreamExt, Sink, SinkExt};
use futures::task::{Spawn, SpawnExt};
use futures::channel::mpsc;

use crypto::identity::PublicKey;

use timer::TimerClient;
use timer::utils::sleep_ticks;


use common::conn::{Listener, ConnPair, ConnTransform};
use relay::client::access_control::{AccessControlOp, AccessControl};

#[derive(Debug)]
enum ListenError {
    SleepTicksError,
    SpawnError,
    Canceled,
}

// TODO: conn_encryptor should probably be implemented in a more generic and efficient way.
// Currently it is possible to perform memory DoS by opening many connections and doing the diffie
// hellman part very slowly. (Note the spawn being called inside this function).

/// Encrypt incoming plain connections
///
/// We require this logic because it is inefficient to perform handshake for the connections serially.
/// For example: It is possible that connection A arrives before connection B, but performing
/// handshake for A takes longer than it takes for B.
async fn conn_encryptor<CS,T,S>(mut plain_connections_receiver: mpsc::Receiver<(PublicKey, ConnPair<Vec<u8>, Vec<u8>>)>,
                            encrypt_transform: T,
                            encrypted_connections_sender: CS,
                            mut spawner: S)
where
    CS: Sink<SinkItem=(PublicKey, ConnPair<Vec<u8>, Vec<u8>>)> + Unpin + Clone + Send + 'static,
    T: ConnTransform<OldSendItem=Vec<u8>,OldRecvItem=Vec<u8>,
                     NewSendItem=Vec<u8>,NewRecvItem=Vec<u8>, 
                     Arg=Option<PublicKey>> + Clone + Send + 'static,
    S: Spawn + Clone + Send + 'static,
{
    while let Some((public_key, (sender, receiver))) = await!(plain_connections_receiver.next()) {
        let mut c_encrypt_transform = encrypt_transform.clone();
        let mut c_encrypted_connections_sender = encrypted_connections_sender.clone();
        spawner.spawn(async move {
            match await!(c_encrypt_transform.transform(Some(public_key.clone()), (sender, receiver))) {
                Some(enc_conn_pair) => {
                    if let Err(_e) = await!(c_encrypted_connections_sender.send((public_key, enc_conn_pair))) {
                        error!("conn_encryptor(): Can not send through encrypted_connections_sender");
                    }
                },
                None => error!("Error encrypting the channel"),
            }
        }).unwrap();
    }
}

#[derive(Debug)]
enum ListenLoopEvent {
    AccessControlOp(Option<AccessControlOp>),
    Connection(Option<(PublicKey, ConnPair<Vec<u8>,Vec<u8>>)>),
}


#[derive(Clone)]
pub struct ChannelerListener<A,L,T,S> {
    client_listener: L,
    encrypt_transform: T,
    address: A,
    backoff_ticks: usize,
    timer_client: TimerClient,
    spawner: S,
}

impl<A,L,T,S> ChannelerListener<A,L,T,S>
where
    A: Clone + Send + Sync + 'static,
    L: Listener<Connection=(PublicKey, ConnPair<Vec<u8>,Vec<u8>>), 
        Config=AccessControlOp, Arg=(A, AccessControl)> + Clone + 'static,
    T: ConnTransform<OldSendItem=Vec<u8>,OldRecvItem=Vec<u8>,
                     NewSendItem=Vec<u8>,NewRecvItem=Vec<u8>, 
                     Arg=Option<PublicKey>> + Clone + Send + 'static,
    S: Spawn + Clone + Send + 'static,
{

    pub fn new(client_listener: L,
           encrypt_transform: T,
           address: A,
           backoff_ticks: usize,
           timer_client: TimerClient,
           spawner: S) -> ChannelerListener<A,L,T,S> {

        ChannelerListener {
            client_listener,
            encrypt_transform,
            address,
            backoff_ticks,
            timer_client,
            spawner,
        }
    }

    async fn listen_iter<'a>(&'a mut self,
                               mut plain_connections_sender: mpsc::Sender<(PublicKey, ConnPair<Vec<u8>, Vec<u8>>)>,
                               mut access_control_sender: mpsc::Sender<AccessControlOp>,
                               access_control_receiver: &'a mut mpsc::Receiver<AccessControlOp>,
                               mut connections_receiver: mpsc::Receiver<(PublicKey, ConnPair<Vec<u8>, Vec<u8>>)>,
                               access_control: &'a mut AccessControl) -> Option<()> {
        loop {
            // We select over .next() invocations instead of selecting over streams because
            // we can't afford to lose access_control_receiver. access_control_receiver lives for the entire outer loop,
            // and as a receiver it can not be cloned.
            // TODO: Find out if we can accidentally lose an access_control_op here.
            // See: https://users.rust-lang.org/t/possibly-losing-an-item-when-using-select-futures-0-3/22961
            let event = select! {
                opt_access_control_op = access_control_receiver.next().fuse() 
                    => ListenLoopEvent::AccessControlOp(opt_access_control_op),
                opt_conn = connections_receiver.next().fuse() => ListenLoopEvent::Connection(opt_conn),
            };
            match event {
                ListenLoopEvent::AccessControlOp(Some(access_control_op)) => {
                    access_control.apply_op(access_control_op.clone());
                    if let Err(_) = await!(access_control_sender.send(access_control_op)) {
                        break;
                    }
                },
                ListenLoopEvent::AccessControlOp(None) => return None,
                ListenLoopEvent::Connection(Some(connection)) => {
                    await!(plain_connections_sender.send(connection)).unwrap();
                },
                ListenLoopEvent::Connection(None) => break,
            }
        }
        Some(())
    }

    async fn listen_loop(&mut self, relay_address: A,
                   mut access_control_receiver: mpsc::Receiver<AccessControlOp>,
                   connections_sender: mpsc::Sender<(PublicKey, ConnPair<Vec<u8>,Vec<u8>>)>,
                   mut access_control: AccessControl)
                    -> Result<!, ListenError> {

        let (mut plain_connections_sender, plain_connections_receiver) = mpsc::channel(0);
        self.spawner.spawn(conn_encryptor(plain_connections_receiver, 
                                          self.encrypt_transform.clone(),
                                          connections_sender.clone(), // Sends encrypted connections
                                          self.spawner.clone()))
            .map_err(|_| ListenError::SpawnError)?;

        loop {

            let (mut access_control_sender, mut connections_receiver) = 
                self.client_listener.clone().listen((relay_address.clone(), access_control.clone()));

            await!(self.listen_iter(plain_connections_sender.clone(),
                                          access_control_sender,
                                          &mut access_control_receiver, 
                                          connections_receiver, 
                                          &mut access_control))
                .ok_or(ListenError::Canceled)?;

            // Wait for a while before attempting to connect again:
            // TODO: Possibly wait here in a smart way? Exponential backoff?
            await!(sleep_ticks(self.backoff_ticks, self.timer_client.clone()))
                .map_err(|_| ListenError::SleepTicksError)?;
        }
    }
}

impl<A,L,T,S> Listener for ChannelerListener<A,L,T,S> 
where
    A: Clone + Send + Sync + 'static,
    L: Listener<Connection=(PublicKey, ConnPair<Vec<u8>,Vec<u8>>), 
        Config=AccessControlOp, Arg=(A, AccessControl)> + Clone + Send + 'static,
    T: ConnTransform<OldSendItem=Vec<u8>,OldRecvItem=Vec<u8>,
                     NewSendItem=Vec<u8>,NewRecvItem=Vec<u8>, 
                     Arg=Option<PublicKey>> + Clone + Send + 'static,
    S: Spawn + Clone + Send + 'static,
{
    type Connection = (PublicKey, ConnPair<Vec<u8>,Vec<u8>>);
    type Config = AccessControlOp;
    type Arg = (A, AccessControl);

    fn listen(mut self, arg: (A, AccessControl)) -> (mpsc::Sender<AccessControlOp>, 
                             mpsc::Receiver<Self::Connection>) {

        let (relay_address, access_control) = arg;

        let (access_control_sender, access_control_receiver) = mpsc::channel(0);
        let (connections_sender, connections_receiver) = mpsc::channel(0);

        let mut spawner = self.spawner.clone();

        let listen_loop_fut = async move {
            await!(self.listen_loop(relay_address,
                   access_control_receiver,
                   connections_sender,
                   access_control)
            .map_err(|e| error!("listen_loop() error: {:?}", e))
            .map(|_| ()))
        };

        // A failure will be detected when the user of this listener
        // tries to read from connection_receiver:
        let _ = spawner.spawn(listen_loop_fut);

        (access_control_sender, connections_receiver)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::executor::ThreadPool;

    use common::conn::IdentityConnTransform;
    use common::dummy_listener::DummyListener;

    use crypto::identity::PUBLIC_KEY_LEN;

    use timer::{create_timer_incoming, dummy_timer_multi_sender, TimerTick};

    async fn task_channeler_listener_basic<S>(spawner: S)
    where
        S: Spawn + Clone + Send + 'static,
    {
        // Create a mock time service:
        let (_tick_sender, tick_receiver) = mpsc::channel::<()>(0);
        let timer_client = create_timer_incoming(tick_receiver, spawner.clone()).unwrap();

        let backoff_ticks = 2;

        let (req_sender, mut req_receiver) = mpsc::channel(0);
        let client_listener = DummyListener::new(req_sender, spawner.clone());

        let public_key_b = PublicKey::from(&[0xbb; PUBLIC_KEY_LEN]);
        let public_key_c = PublicKey::from(&[0xcc; PUBLIC_KEY_LEN]);

        // We don't need encryption for this test:
        let encrypt_transform = IdentityConnTransform::<Vec<u8>,Vec<u8>,Option<PublicKey>>::new();
        let relay_address = 0x1337u32;

        let channeler_listener = ChannelerListener::new(
            client_listener,
            encrypt_transform,
            relay_address,
            backoff_ticks,
            timer_client,
            spawner.clone());

        let access_control = AccessControl::new();
        let (mut access_control_sender, mut connections_receiver) = channeler_listener.listen((relay_address, access_control));

        // Inner listener:
        let client_listener_fut = async {
            let mut listen_request = await!(req_receiver.next()).unwrap();
            let (ref arg_relay_address, _) = listen_request.arg;
            assert_eq!(arg_relay_address, &relay_address);

            // Receive exmaple configuration message:
            let access_control_op = await!(listen_request.config_receiver.next()).unwrap();
            assert_eq!(access_control_op, AccessControlOp::Add(public_key_b.clone()));

            // Provide first connection:
            let (mut local_sender, remote_receiver) = mpsc::channel(0);
            let (remote_sender, _local_receiver) = mpsc::channel(0);
            await!(listen_request.conn_sender.send(
                    (public_key_b.clone(), (remote_sender, remote_receiver)))).unwrap();
            await!(local_sender.send(vec![1,2,3])).unwrap();

            // Provide second connection:
            let (_local_sender, remote_receiver) = mpsc::channel(0);
            let (remote_sender, mut local_receiver) = mpsc::channel(0);
            await!(listen_request.conn_sender.send(
                    (public_key_c.clone(), (remote_sender, remote_receiver)))).unwrap();
            assert_eq!(await!(local_receiver.next()).unwrap(), vec![3,2,1]);
        };

        // Wrapper listener:
        let channeler_listener_fut = async {
            // Send example configuration message:
            await!(access_control_sender.send(AccessControlOp::Add(public_key_b.clone()))).unwrap();

            // Get first connection:
            let (public_key, (_sender, mut receiver)) = await!(connections_receiver.next()).unwrap();
            assert_eq!(public_key, public_key_b);
            assert_eq!(await!(receiver.next()).unwrap(), vec![1,2,3]);

            // Get second connection:
            let (public_key, (mut sender, _receiver)) = await!(connections_receiver.next()).unwrap();
            assert_eq!(public_key, public_key_c);
            await!(sender.send(vec![3,2,1])).unwrap();

        };

        // Run inner listener and wrapper listener at the same time:
        await!(client_listener_fut.join(channeler_listener_fut));
    }

    #[test]
    fn test_channeler_listener_basic() {
        let mut thread_pool = ThreadPool::new().unwrap();
        thread_pool.run(task_channeler_listener_basic(thread_pool.clone()));
    }


    async fn task_channeler_listener_retry<S>(spawner: S)
    where
        S: Spawn + Clone + Send + 'static,
    {
        // Create a mock time service:
        let (mut tick_sender_receiver, timer_client) = dummy_timer_multi_sender(spawner.clone());

        let backoff_ticks = 2;

        let (req_sender, mut req_receiver) = mpsc::channel(0);
        let client_listener = DummyListener::new(req_sender, spawner.clone());

        let public_key_b = PublicKey::from(&[0xbb; PUBLIC_KEY_LEN]);
        let public_key_c = PublicKey::from(&[0xcc; PUBLIC_KEY_LEN]);

        // We don't need encryption for this test:
        let encrypt_transform = IdentityConnTransform::<Vec<u8>,Vec<u8>,Option<PublicKey>>::new();
        let relay_address = 0x1337u32;

        let channeler_listener = ChannelerListener::new(
            client_listener,
            encrypt_transform,
            relay_address,
            backoff_ticks,
            timer_client,
            spawner.clone());

        let access_control = AccessControl::new();
        let (_access_control_sender, mut connections_receiver) = channeler_listener.listen((relay_address, access_control));

        // Inner listener:
        let client_listener_fut = async {
            let mut listen_request = await!(req_receiver.next()).unwrap();
            let (ref arg_relay_address, _) = listen_request.arg;
            assert_eq!(arg_relay_address, &relay_address);

            // Provide first connection:
            let (_local_sender, remote_receiver) = mpsc::channel(0);
            let (remote_sender, _local_receiver) = mpsc::channel(0);
            await!(listen_request.conn_sender.send(
                    (public_key_b.clone(), (remote_sender, remote_receiver)))).unwrap();

            // Simulate losing connection to relay:
            drop(listen_request);

            // ChannelerListener will wait `backoff_ticks` before attempting to reconnect
            // to the relay:
            let mut tick_sender = await!(tick_sender_receiver.next()).unwrap();
            for _ in 0 .. backoff_ticks {
                await!(tick_sender.send(TimerTick)).unwrap();
            }

            // We expect that ChannelerListener will now attempt to reconnect to relay:
            let mut listen_request = await!(req_receiver.next()).unwrap();
            let (ref arg_relay_address, _) = listen_request.arg;
            assert_eq!(arg_relay_address, &relay_address);

            // Provide second connection:
            let (_local_sender, remote_receiver) = mpsc::channel(0);
            let (remote_sender, _local_receiver) = mpsc::channel(0);
            await!(listen_request.conn_sender.send(
                    (public_key_c.clone(), (remote_sender, remote_receiver)))).unwrap();
        };

        // Wrapper listener:
        let channeler_listener_fut = async {
            // Get first connection:
            let (public_key, (_sender, _receiver)) = await!(connections_receiver.next()).unwrap();
            assert_eq!(public_key, public_key_b);

            // Get second connection:
            let (public_key, (_sender, _receiver)) = await!(connections_receiver.next()).unwrap();
            assert_eq!(public_key, public_key_c);
        };

        // Run inner listener and wrapper listener at the same time:
        await!(client_listener_fut.join(channeler_listener_fut));
    }

    #[test]
    fn test_channeler_listener_retry() {
        let mut thread_pool = ThreadPool::new().unwrap();
        thread_pool.run(task_channeler_listener_retry(thread_pool.clone()));
    }
}


