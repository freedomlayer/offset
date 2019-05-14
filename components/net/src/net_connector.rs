use common::conn::{BoxFuture, ConnPairVec, FutTransform};
use futures::task::Spawn;

use proto::net::messages::NetAddress;

use crate::resolver::Resolver;
use crate::tcp_connector::TcpConnector;

#[derive(Clone)]
pub struct NetConnector<S, RS> {
    resolver: Resolver<RS>,
    tcp_connector: TcpConnector<S>,
}

impl<S, RS> NetConnector<S, RS> {
    pub fn new(max_frame_length: usize, resolve_spawner: RS, spawner: S) -> Self {
        NetConnector {
            resolver: Resolver::new(resolve_spawner),
            tcp_connector: TcpConnector::new(max_frame_length, spawner),
        }
    }
}

impl<S, RS> FutTransform for NetConnector<S, RS>
where
    S: Spawn + Send,
    RS: Spawn + Send,
{
    type Input = NetAddress;
    type Output = Option<ConnPairVec>;

    fn transform(&mut self, net_address: Self::Input) -> BoxFuture<'_, Self::Output> {
        debug!("Connecting to {:?}", net_address);
        Box::pin(async move {
            let socket_addr_vec = await!(self.resolver.transform(net_address));
            // A trivial implementation: We try to connect to the first address on the list.
            // TODO: Maybe choose a random address in the future?
            let socket_addr = socket_addr_vec.get(0)?;
            await!(self.tcp_connector.transform(*socket_addr))
        })
    }
}
