use crypto::identity::PublicKey;
use std::marker::PhantomData;

trait Connector {
    type Address;

    fn connect(address: Self::Address) -> (impl AsyncWrite, impl AsyncRead);
}

pub struct RelayConnector<A> {
    phantom_address: PhantomData<A>,
}

impl<A> RelayConnector<A> {
    pub fn new(connector: ()) -> RelayConnector<A> {
        RelayConnector {
            phantom_address: PhantomData,
        }
    }

    pub async fn connect(relay_address: A, remote_public_key: PublicKey) {
    }
}

async fn relay_client() {
}
