use std::net::{SocketAddr, ToSocketAddrs};

use futures::future;
use futures::task::{Spawn, SpawnExt};

use common::conn::{BoxFuture, FutTransform};
use proto::net::messages::NetAddress;

#[derive(Clone)]
pub struct Resolver<S> {
    spawner: S,
}

impl<S> Resolver<S> {
    pub fn new(spawner: S) -> Self {
        Resolver { spawner }
    }
}

impl<S> FutTransform for Resolver<S>
where
    S: Spawn,
{
    type Input = NetAddress;
    type Output = Vec<SocketAddr>;

    fn transform(&mut self, net_address: Self::Input) -> BoxFuture<'_, Self::Output> {
        let resolve_fut = future::lazy(move |_| {
            if let Ok(socket_addr_iter) = net_address.as_str().to_socket_addrs() {
                socket_addr_iter.collect::<Vec<_>>()
            } else {
                Vec::new()
            }
        });
        match self.spawner.spawn_with_handle(resolve_fut) {
            Ok(resolve_res_fut) => Box::pin(resolve_res_fut),
            Err(_) => Box::pin(future::ready(Vec::new())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::convert::TryInto;
    use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

    use futures::executor::{LocalPool, ThreadPool};

    #[test]
    fn test_resolver_numeric_v4() {
        let net_thread_pool = ThreadPool::new().unwrap();
        let mut resolver = Resolver::new(net_thread_pool);
        let res_vec = LocalPool::new()
            .run_until(resolver.transform("127.0.0.1:1337".to_owned().try_into().unwrap()));

        let loopback = Ipv4Addr::new(127, 0, 0, 1);
        let socket_addr = SocketAddr::new(IpAddr::V4(loopback), 1337);
        assert_eq!(res_vec, vec![socket_addr]);
    }

    #[test]
    fn test_resolver_numeric_v6() {
        let net_thread_pool = ThreadPool::new().unwrap();
        let mut resolver = Resolver::new(net_thread_pool);
        let res_vec = LocalPool::new()
            .run_until(resolver.transform("::1:1338".to_owned().try_into().unwrap()));

        let loopback = Ipv6Addr::new(0, 0, 0, 0, 0, 0, 0, 1);
        let socket_addr = SocketAddr::new(IpAddr::V6(loopback), 1338);
        assert_eq!(res_vec, vec![socket_addr]);
    }

    #[test]
    fn test_resolver_localhost_v4() {
        let net_thread_pool = ThreadPool::new().unwrap();
        let mut resolver = Resolver::new(net_thread_pool);
        let res_vec = LocalPool::new()
            .run_until(resolver.transform("localhost:1339".to_owned().try_into().unwrap()));

        let loopback = Ipv4Addr::new(127, 0, 0, 1);
        let socket_addr = SocketAddr::new(IpAddr::V4(loopback), 1339);
        assert!(res_vec.contains(&socket_addr));
    }
}
