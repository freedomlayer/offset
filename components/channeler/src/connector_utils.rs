use futures::task::Spawn;

use crypto::identity::PublicKey;
use crypto::crypto_rand::CryptoRandom;
use timer::TimerClient;
use identity::IdentityClient;
use common::connector::{BoxFuture, Connector, ConnPair};
use secure_channel::create_secure_channel;

/// A wrapper for a connector.
/// Always connects to the same address.
#[derive(Clone)]
pub struct ConstAddressConnector<C,A> {
    connector: C,
    address: A,
}

impl<C,A> ConstAddressConnector<C,A> {
    pub fn new(connector: C, address: A) -> ConstAddressConnector<C,A> {
        ConstAddressConnector {
            connector,
            address,
        }
    }
}


impl<C,A> Connector for ConstAddressConnector<C,A>
where
    C: Connector<Address=A>,
    A: Clone,
{
    type Address = ();
    type SendItem = C::SendItem;
    type RecvItem = C::RecvItem;

    fn connect(&mut self, _address: ()) 
        -> BoxFuture<'_, Option<ConnPair<C::SendItem, C::RecvItem>>> {
        self.connector.connect(self.address.clone())
    }
}



#[derive(Clone)]
pub struct EncryptedConnector<C,R,S> {
    connector: C,
    identity_client: IdentityClient,
    rng: R,
    timer_client: TimerClient,
    ticks_to_rekey: usize,
    spawner: S,
}

/// Turns a connector into a connector that yields encrypted connections.
/// Addresses are changed from A into (PublicKey, A), 
/// where public_key is the identity of the remot side.
impl<C,R,S> EncryptedConnector<C,R,S> {
    #[allow(unused)]
    pub fn new(connector: C, 
               identity_client: IdentityClient,
               rng: R,
               timer_client: TimerClient,
               ticks_to_rekey: usize,
               spawner: S) -> EncryptedConnector<C,R,S> {

        EncryptedConnector {
            connector,
            identity_client,
            rng,
            timer_client,
            ticks_to_rekey,
            spawner,
        }
    }
}

impl<A,C,R,S> Connector for EncryptedConnector<C,R,S>
where
    R: CryptoRandom + 'static,
    C: Connector<Address=A,SendItem=Vec<u8>,RecvItem=Vec<u8>> + Send,
    A: Clone + Send + 'static,
    S: Spawn + Clone + Send,
{
    type Address = (PublicKey, A);
    type SendItem = Vec<u8>;
    type RecvItem = Vec<u8>;

    fn connect(&mut self, full_address: (PublicKey, A))
        -> BoxFuture<'_, Option<ConnPair<C::SendItem, C::RecvItem>>> {

        let (public_key, address) = full_address;
        let fut = async move {
            let conn_pair = await!(self.connector.connect(address))?;
            let (sender, receiver) = await!(create_secure_channel(
                                      conn_pair.sender, conn_pair.receiver, 
                                      self.identity_client.clone(),
                                      Some(public_key.clone()),
                                      self.rng.clone(),
                                      self.timer_client.clone(),
                                      self.ticks_to_rekey,
                                      self.spawner.clone()))
                                    .ok()?;
            Some(ConnPair { sender, receiver })
        };
        Box::pinned(fut)
    }
}
