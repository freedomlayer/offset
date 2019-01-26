use futures::task::{Spawn, SpawnExt};
use futures::channel::mpsc;
use futures::{StreamExt, SinkExt};

use common::conn::{ConnPairVec, FutTransform, BoxFuture};

use crypto::identity::PublicKey;
use index_client::ServerConn;

use proto::funder::messages::{RelayAddress, TcpAddress};
use proto::index_server::messages::{IndexServerAddress};
use proto::index_server::serialize::{serialize_index_client_to_server,
        deserialize_index_server_to_client};

#[derive(Clone)]
/// Open an encrypted connection to a relay endpoint
pub struct EncRelayConnector<ET,C> {
    encrypt_transform: ET,
    net_connector: C,
}

impl<ET,C> EncRelayConnector<ET,C> {
    pub fn new(encrypt_transform: ET,
               net_connector: C) -> Self {

        EncRelayConnector {
            encrypt_transform,
            net_connector,
        }
    }
}

impl<ET,C> FutTransform for EncRelayConnector<ET,C> 
where
    C: FutTransform<Input=TcpAddress,Output=Option<ConnPairVec>> + Clone + Send,
    ET: FutTransform<Input=(Option<PublicKey>, ConnPairVec),Output=Option<(PublicKey, ConnPairVec)>> + Send,
{
    type Input = RelayAddress;
    type Output = Option<ConnPairVec>;

    fn transform(&mut self, relay_address: Self::Input)
        -> BoxFuture<'_, Self::Output> {

        Box::pin(async move {
            let conn_pair = await!(self.net_connector.transform(relay_address.address))?;
            let (_public_key, conn_pair) = await!(self.encrypt_transform.transform((Some(relay_address.public_key), conn_pair)))?;
            Some(conn_pair)
        })
    }
}

/// A connection style encrypt transform.
/// Does not return the public key of the remote side, because we already know it.
#[derive(Clone)]
pub struct ConnectEncryptTransform<ET> {
    encrypt_transform: ET,
}

impl<ET> ConnectEncryptTransform<ET> {
    pub fn new(encrypt_transform: ET) -> Self {

        ConnectEncryptTransform {
            encrypt_transform,
        }
    }
}

impl<ET> FutTransform for ConnectEncryptTransform<ET> 
where
    ET: FutTransform<Input=(Option<PublicKey>, ConnPairVec),Output=Option<(PublicKey, ConnPairVec)>> + Send,
{
    type Input = (PublicKey, ConnPairVec);
    type Output = Option<ConnPairVec>;

    fn transform(&mut self, input: Self::Input)
        -> BoxFuture<'_, Self::Output> {

        let (public_key, conn_pair) = input;

        Box::pin(async move {
            let (_public_key, conn_pair) = await!(
                self.encrypt_transform.transform((Some(public_key), conn_pair)))?;
            Some(conn_pair)
        })
    }
}

/// A Listen style encrypt transform.
/// Returns the public key of the remote side, because we can not predict it.
#[derive(Clone)]
pub struct ListenEncryptTransform<ET> {
    encrypt_transform: ET,
}

impl<ET> ListenEncryptTransform<ET> {
    pub fn new(encrypt_transform: ET) -> Self {

        ListenEncryptTransform {
            encrypt_transform,
        }
    }
}

impl<ET> FutTransform for ListenEncryptTransform<ET> 
where
    ET: FutTransform<Input=(Option<PublicKey>, ConnPairVec),Output=Option<(PublicKey, ConnPairVec)>> + Send,
{
    type Input = (PublicKey, ConnPairVec);
    type Output = Option<(PublicKey, ConnPairVec)>;

    fn transform(&mut self, input: Self::Input)
        -> BoxFuture<'_, Self::Output> {

        let (public_key, conn_pair) = input;

        Box::pin(async move {
            await!(self.encrypt_transform.transform((Some(public_key), conn_pair)))
        })
    }
}


// C: FutTransform<Input=IndexServerAddress, Output=Option<ServerConn>> + Send,
#[derive(Clone)]
/// Connect to an index server
pub struct EncIndexClientConnector<ET,KT,C,S> {
    encrypt_transform: ET,
    keepalive_transform: KT,
    net_connector: C,
    spawner: S,
}


impl<ET,KT,C,S> EncIndexClientConnector<ET,KT,C,S> {
    pub fn new(encrypt_transform: ET,
               keepalive_transform: KT,
               net_connector: C,
               spawner: S) -> Self {

        EncIndexClientConnector {
            encrypt_transform,
            keepalive_transform,
            net_connector,
            spawner,
        }
    }
}

impl<ET,KT,C,S> FutTransform for EncIndexClientConnector<ET,KT,C,S> 
where
    ET: FutTransform<Input=(Option<PublicKey>, ConnPairVec),Output=Option<(PublicKey, ConnPairVec)>> + Send,
    KT: FutTransform<Input=ConnPairVec,Output=ConnPairVec> + Send,
    C: FutTransform<Input=TcpAddress,Output=Option<ConnPairVec>> + Clone + Send,
    S: Spawn + Send,
{
    type Input = IndexServerAddress;
    type Output = Option<ServerConn>;

    fn transform(&mut self, index_server_address: Self::Input)
        -> BoxFuture<'_, Self::Output> {

        Box::pin(async move {
            let conn_pair = await!(self.net_connector.transform(index_server_address.address))?;
            let (_public_key, conn_pair) = await!(self.encrypt_transform.transform((Some(index_server_address.public_key), conn_pair)))?;
            let (mut data_sender, mut data_receiver) = await!(self.keepalive_transform.transform(conn_pair));

            let (user_sender, mut local_receiver) = mpsc::channel(0);
            let (mut local_sender, user_receiver) = mpsc::channel(0);

            // Deserialize incoming data:
            let deser_fut = async move {
                while let Some(data) = await!(data_receiver.next()) {
                    let message = match deserialize_index_server_to_client(&data) {
                        Ok(message) => message,
                        Err(_) => return,
                    };
                    if let Err(_) = await!(local_sender.send(message)) {
                        return;
                    }
                }
            };
            // If there is any error here, the user will find out when
            // he tries to read from `user_receiver`
            let _ = self.spawner.spawn(deser_fut);

            // Serialize outgoing data:
            let ser_fut = async move {
                while let Some(message) = await!(local_receiver.next()) {
                    let data = serialize_index_client_to_server(&message);
                    if let Err(_) = await!(data_sender.send(data)) {
                        return;
                    }
                }
            };
            // If there is any error here, the user will find out when
            // he tries to send through `user_sender`
            let _ = self.spawner.spawn(ser_fut);

            Some((user_sender, user_receiver))
        })
    }
}

