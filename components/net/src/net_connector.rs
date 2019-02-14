use common::conn::{FutTransform, BoxFuture, ConnPairVec};
use futures::task::Spawn;

use proto::net::messages::NetAddress;

use crate::tcp_connector::TcpConnector;
use crate::resolver::{Resolver, ResolverError};

#[derive(Debug)]
pub enum NetConnectorError {
    ResolverError(ResolverError),
}

impl From<ResolverError> for NetConnectorError {
    fn from(e: ResolverError) -> Self {
        NetConnectorError::ResolverError(e)
    }
}


#[derive(Clone)]
pub struct NetConnector<S> {
    resolver: Resolver,
    tcp_connector: TcpConnector<S>,
}

impl<S> NetConnector<S> {
    #[allow(unused)]
    pub fn new(max_frame_length: usize,
           spawner: S) -> Result<Self, NetConnectorError> {

        Ok(NetConnector {
            resolver: Resolver::new()?,
            tcp_connector: TcpConnector::new(max_frame_length, spawner),
        })
    }
}

impl<S> FutTransform for NetConnector<S> 
where
    S: Spawn + Send,
{
    type Input = NetAddress;
    type Output = Option<ConnPairVec>;

    fn transform(&mut self, net_address: Self::Input)
        -> BoxFuture<'_, Self::Output> {

        Box::pin(async move {
            let socket_addr_vec = await!(self.resolver.transform(net_address));
            // A trivial implementation: We try to connect to the first address on the list.
            // TODO: Maybe choose a random address in the future?
            let socket_addr = socket_addr_vec.get(0)?.clone();
            await!(self.tcp_connector.transform(socket_addr))
        })
    }
}
