use futures::{TryFutureExt, FutureExt};
use futures::task::{Spawn, SpawnExt};
use futures::channel::{mpsc, oneshot};

use crypto::identity::PublicKey;
use crypto::crypto_rand::CryptoRandom;

use identity::IdentityClient;

use common::conn::{BoxFuture, FutTransform};

use crate::single_client::{ServerConn, SingleClientControl,
                        first_server_time_hash, single_client_loop, 
                        SingleClientError};

pub type ControlSender = mpsc::Sender<SingleClientControl>;
pub type CloseReceiver = oneshot::Receiver<Result<(), SingleClientError>>;
pub type SessionHandle = (ControlSender, CloseReceiver);

#[derive(Clone)]
pub struct IndexClientSession<C,R,S> {
    connector: C,
    local_public_key: PublicKey,
    identity_client: IdentityClient,
    rng: R,
    spawner: S,
}

impl<ISA,C,R,S> IndexClientSession<C,R,S> 
where
    ISA: 'static,
    C: FutTransform<Input=ISA, Output=Option<ServerConn>>,
    R: CryptoRandom + 'static,
    S: Spawn,
{

    pub fn new(connector: C,
               local_public_key: PublicKey,
               identity_client: IdentityClient,
               rng: R,
               spawner: S) -> Self {

        IndexClientSession {
            connector,
            local_public_key,
            identity_client,
            rng,
            spawner,
        }
    }

    async fn connect(&mut self, index_server_address: ISA) -> Option<SessionHandle> {
        let (to_server, mut from_server) = await!(self.connector.transform(index_server_address))?;

        let first_time_hash = await!(first_server_time_hash(&mut from_server)).ok()?;
        let (control_sender, incoming_control) = mpsc::channel(0);

        let (close_sender, close_receiver) = oneshot::channel();

        let single_client_fut = single_client_loop((to_server, from_server),
                           incoming_control,
                           self.local_public_key.clone(),
                           self.identity_client.clone(),
                           self.rng.clone(),
                           first_time_hash)
            .map(|res| { 
                if let Err(res) = close_sender.send(res) {
                    error!("Failed to send result from single_client_loop(): {:?}", res);
                }
            });

        self.spawner.spawn(single_client_fut).ok()?;
        Some((control_sender, close_receiver))
    }
}

impl<ISA,C,R,S> FutTransform for IndexClientSession<C,R,S> 
where
    ISA: Send + 'static,
    C: FutTransform<Input=ISA, Output=Option<ServerConn>> + Send,
    S: Spawn + Send,
    R: CryptoRandom + 'static,
{
    /// Address of an index server
    type Input = ISA;
    /// A pair: control sender, and a receiver that notifies about disconnection.
    type Output = Option<SessionHandle>;

    fn transform(&mut self, index_server_address: Self::Input) 
        -> BoxFuture<'_, Self::Output> {

        Box::pinned(self.connect(index_server_address))
    }
}

