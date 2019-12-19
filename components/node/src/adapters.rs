use futures::task::Spawn;

use common::conn::{BoxFuture, ConnPairVec, FutTransform};

use proto::crypto::PublicKey;

// use proto::app_server::messages::RelayAddress;
use proto::index_server::messages::IndexServerAddress;
use proto::net::messages::NetAddress;

/*
#[derive(Clone)]
/// Open an encrypted connection to a relay endpoint
pub struct EncRelayConnector<ET, C> {
    encrypt_transform: ET,
    net_connector: C,
}

impl<ET, C> EncRelayConnector<ET, C> {
    pub fn new(encrypt_transform: ET, net_connector: C) -> Self {
        EncRelayConnector {
            encrypt_transform,
            net_connector,
        }
    }
}

impl<ET, C> FutTransform for EncRelayConnector<ET, C>
where
    C: FutTransform<Input = NetAddress, Output = Option<ConnPairVec>> + Clone + Send,
    ET: FutTransform<
            Input = (Option<PublicKey>, ConnPairVec),
            Output = Option<(PublicKey, ConnPairVec)>,
        > + Send,
{
    type Input = RelayAddress;
    type Output = Option<ConnPairVec>;

    fn transform(&mut self, relay_address: Self::Input) -> BoxFuture<'_, Self::Output> {
        Box::pin(async move {
            let conn_pair = self.net_connector.transform(relay_address.address).await?;
            let (_public_key, conn_pair) = self
                .encrypt_transform
                .transform((Some(relay_address.public_key), conn_pair))
                .await?;
            Some(conn_pair)
        })
    }
}

#[derive(Clone)]
pub struct EncKeepaliveConnector<ET, KT, C, S> {
    encrypt_transform: ET,
    keepalive_transform: KT,
    net_connector: C,
    spawner: S,
}

impl<ET, KT, C, S> EncKeepaliveConnector<ET, KT, C, S> {
    pub fn new(
        encrypt_transform: ET,
        keepalive_transform: KT,
        net_connector: C,
        spawner: S,
    ) -> Self {
        EncKeepaliveConnector {
            encrypt_transform,
            keepalive_transform,
            net_connector,
            spawner,
        }
    }
}

impl<ET, KT, C, S> FutTransform for EncKeepaliveConnector<ET, KT, C, S>
where
    ET: FutTransform<
            Input = (Option<PublicKey>, ConnPairVec),
            Output = Option<(PublicKey, ConnPairVec)>,
        > + Send,
    KT: FutTransform<Input = ConnPairVec, Output = ConnPairVec> + Send,
    C: FutTransform<Input = NetAddress, Output = Option<ConnPairVec>> + Clone + Send,
    S: Spawn + Send,
{
    type Input = IndexServerAddress<NetAddress>;
    type Output = Option<ConnPairVec>;

    fn transform(&mut self, index_server_address: Self::Input) -> BoxFuture<'_, Self::Output> {
        Box::pin(async move {
            let conn_pair = self
                .net_connector
                .transform(index_server_address.address)
                .await?;
            let (_public_key, conn_pair) = self
                .encrypt_transform
                .transform((Some(index_server_address.public_key), conn_pair))
                .await?;
            Some(self.keepalive_transform.transform(conn_pair).await)
        })
    }
}
*/
