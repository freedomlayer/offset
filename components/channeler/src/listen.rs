use std::marker::Unpin;
use futures::{select, FutureExt, TryFutureExt, Stream, StreamExt, Sink, SinkExt};
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
}

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

    fn new(client_listener: L,
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

    async fn listen_loop(&mut self, relay_address: A,
                   mut access_control_receiver: mpsc::Receiver<AccessControlOp>,
                   connections_sender: mpsc::Sender<(PublicKey, ConnPair<Vec<u8>,Vec<u8>>)>,
                   mut access_control: AccessControl)
                    -> Result<!, ListenError> {

        loop {
            let (mut plain_connections_sender, plain_connections_receiver) = mpsc::channel(0);
            self.spawner.spawn(conn_encryptor(plain_connections_receiver, 
                                              self.encrypt_transform.clone(),
                                              connections_sender.clone(), // Sends encrypted connections
                                              self.spawner.clone()))
                .map_err(|_| ListenError::SpawnError)?;

            let (mut access_control_sender, mut connections_receiver) = 
                self.client_listener.clone().listen((relay_address.clone(), access_control.clone()));

            let fut_access_control_send_all = self.spawner.spawn_with_handle(async move {
                while let Some(access_control_op) = await!(access_control_receiver.next()) {
                    access_control.apply_op(access_control_op.clone());
                    if let Err(_) = await!(access_control_sender.send(access_control_op)) {
                        break;
                    }
                }
                (access_control, access_control_receiver)
            }).map_err(|_| ListenError::SpawnError)?;

            let fut_connections_send_all = 
                plain_connections_sender.send_all(&mut connections_receiver);

            let ((res_access_control, res_access_control_receiver), _ ) = 
                await!(fut_access_control_send_all.join(fut_connections_send_all));

            access_control = res_access_control;
            access_control_receiver = res_access_control_receiver;

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

    use timer::{create_timer_incoming, dummy_timer_multi_sender, TimerTick};

    async fn task_channeler_listener_basic<S>(spawner: S)
    where
        S: Spawn + Clone + Send + 'static,
    {
        // Create a mock time service:
        let (_tick_sender, tick_receiver) = mpsc::channel::<()>(0);
        let timer_client = create_timer_incoming(tick_receiver, spawner.clone()).unwrap();

        let backoff_ticks = 2;

        // Create dummy listener here?
        // let client_listener = DummyListener::new(...)

        let (req_sender, req_receiver) = mpsc::channel(0);
        let client_listener = DummyListener::new(req_sender, spawner.clone());
        client_listener.clone();

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

        // TODO: Continue here.

    }

    #[test]
    fn test_channeler_listener_basic() {
        let mut thread_pool = ThreadPool::new().unwrap();
        thread_pool.run(task_channeler_listener_basic(thread_pool.clone()));
    }
}


