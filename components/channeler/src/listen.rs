use std::marker::Unpin;
use futures::{select, FutureExt, TryFutureExt, Stream, StreamExt, Sink, SinkExt};
use futures::task::{Spawn, SpawnExt};
use futures::channel::mpsc;

use proto::consts::TICKS_TO_REKEY;

use crypto::identity::PublicKey;
use crypto::crypto_rand::CryptoRandom;
use timer::TimerClient;
use timer::utils::sleep_ticks;

use identity::IdentityClient;

use common::conn::{Listener, ConnPair};
use relay::client::access_control::{AccessControlOp, AccessControl};

use secure_channel::create_secure_channel;

#[derive(Debug)]
pub enum ListenError {
    SleepTicksError,
    SpawnError,
}

/// Encrypt incoming plain connections
///
/// We require this logic because it is inefficient to perform handshake for the connections serially.
/// For example: It is possible that connection A arrives before connection B, but performing
/// handshake for A takes longer than it takes for B.
pub async fn conn_encryptor<CS,R,S>(mut plain_connections_receiver: mpsc::Receiver<(PublicKey, ConnPair<Vec<u8>, Vec<u8>>)>,
                            encrypted_connections_sender: CS,
                            timer_client: TimerClient,
                            identity_client: IdentityClient,
                            rng: R,
                            mut spawner: S)
where
    CS: Sink<SinkItem=(PublicKey, ConnPair<Vec<u8>, Vec<u8>>)> + Unpin + Clone + Send + 'static,
    R: CryptoRandom + 'static,
    S: Spawn + Clone + Send + 'static,
{
    while let Some((public_key, conn_pair)) = await!(plain_connections_receiver.next()) {
        let secure_channel_fut = create_secure_channel(conn_pair.sender, conn_pair.receiver,
                              identity_client.clone(),
                              Some(public_key.clone()),
                              rng.clone(),
                              timer_client.clone(),
                              TICKS_TO_REKEY,
                              spawner.clone());
        let mut c_encrypted_connections_sender = encrypted_connections_sender.clone();
        spawner.spawn(async move {
            match await!(secure_channel_fut) {
                Ok((sender, receiver)) => {
                    let conn_pair = ConnPair {sender, receiver};
                    if let Err(_e) = await!(c_encrypted_connections_sender.send((public_key, conn_pair))) {
                        error!("conn_encryptor(): Can not send through encrypted_connections_sender");
                    }
                },
                Err(e) => 
                    error!("conn_encryptor(): error in create_secure_channel(): {:?}", e),
            }
        }).unwrap();
    }
}


#[derive(Clone)]
pub struct ChannelerListener<A,L,R,S> {
    client_listener: L,
    address: A,
    conn_timeout_ticks: usize,
    keepalive_ticks: usize,
    backoff_ticks: usize,
    timer_client: TimerClient,
    identity_client: IdentityClient,
    rng: R,
    spawner: S,
}

impl<A,L,R,S> ChannelerListener<A,L,R,S>
where
    A: Clone + Send + Sync + 'static,
    L: Listener<Connection=(PublicKey, ConnPair<Vec<u8>,Vec<u8>>), 
        Config=AccessControlOp, Arg=(A, AccessControl)> + Clone + 'static,
    R: CryptoRandom + 'static,
    S: Spawn + Clone + Send + 'static,
{

    fn new() -> ChannelerListener<A,L,R,S> {
        unimplemented!();
    }

    async fn listen_loop(&mut self, relay_address: A,
                   mut access_control_receiver: mpsc::Receiver<AccessControlOp>,
                   connections_sender: mpsc::Sender<(PublicKey, ConnPair<Vec<u8>,Vec<u8>>)>,
                   mut access_control: AccessControl)
                    -> Result<!, ListenError> {

        loop {
            let (mut plain_connections_sender, plain_connections_receiver) = mpsc::channel(0);
            self.spawner.spawn(conn_encryptor(plain_connections_receiver, 
                                         connections_sender.clone(), // Sends encrypted connections
                                         self.timer_client.clone(),
                                         self.identity_client.clone(),
                                         self.rng.clone(),
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

impl<A,L,R,S> Listener for ChannelerListener<A,L,R,S> 
where
    A: Clone + Send + Sync + 'static,
    L: Listener<Connection=(PublicKey, ConnPair<Vec<u8>,Vec<u8>>), 
        Config=AccessControlOp, Arg=(A, AccessControl)> + Clone + Send + 'static,
    R: CryptoRandom + 'static,
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

        // TODO: Is there a less hacky way to do this?:
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


