use common::conn::{BoxFuture, BoxStream, ConnPairVec};
use crypto::dh::DhStaticPrivateKey;

use proto::app_server::messages::RelayAddress;
use proto::crypto::DhPublicKey;

#[derive(Debug)]
struct RelayClientError;

trait RelayClient {
    fn connect(
        &mut self,
        relay: RelayAddress,
        port: DhPublicKey,
    ) -> BoxFuture<'_, Result<ConnPairVec, RelayClientError>>;

    fn listen(
        &mut self,
        relay: RelayAddress,
        port_private: DhStaticPrivateKey,
    ) -> BoxStream<'_, ConnPairVec>;
}
