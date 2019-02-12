use std::net::{SocketAddr, ToSocketAddrs};

use futures::future;
use futures::task::SpawnExt;
use futures::executor::ThreadPool;

use common::conn::{FutTransform, BoxFuture};
use proto::net::messages::NetAddress;

#[derive(Debug)]
pub enum ResolverError {
    CreateThreadPoolError,
}

#[derive(Clone)]
pub struct Resolver {
    thread_pool: ThreadPool,
}

impl Resolver {
    pub fn new() -> Result<Self, ResolverError> {
        Ok(Resolver {
            thread_pool: ThreadPool::new()
                .map_err(|_| ResolverError::CreateThreadPoolError)?,
        })
    }
}

impl FutTransform for Resolver {
    type Input = NetAddress;
    type Output = Vec<SocketAddr>;

    fn transform(&mut self, net_address: Self::Input)
        -> BoxFuture<'_, Self::Output> {

        let resolve_fut = future::lazy(move |_| {
            if let Ok(socket_addr_iter) = net_address.as_str().to_socket_addrs() {
                socket_addr_iter.collect::<Vec<_>>()
            } else {
                Vec::new()
            }
        });
        match self.thread_pool.spawn_with_handle(resolve_fut) {
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
    use futures::executor::ThreadPool;

    #[test]
    fn test_resolver_numeric_v4() {
        let mut thread_pool = ThreadPool::new().unwrap();
        let mut resolver = Resolver::new().unwrap();
        let res_vec = thread_pool.run(resolver.transform("127.0.0.1:1337".to_owned().try_into().unwrap()));

        let loopback = Ipv4Addr::new(127, 0, 0, 1);
        let socket_addr = SocketAddr::new(IpAddr::V4(loopback), 1337);
        assert_eq!(res_vec, vec![socket_addr]);
    }

    #[test]
    fn test_resolver_numeric_v6() {
        let mut thread_pool = ThreadPool::new().unwrap();
        let mut resolver = Resolver::new().unwrap();
        let res_vec = thread_pool.run(resolver.transform("::1:1338".to_owned().try_into().unwrap()));

        let loopback = Ipv6Addr::new(0, 0, 0, 0, 0, 0, 0, 1);
        let socket_addr = SocketAddr::new(IpAddr::V6(loopback), 1338);
        assert_eq!(res_vec, vec![socket_addr]);
    }

    #[test]
    fn test_resolver_localhost_v4() {
        let mut thread_pool = ThreadPool::new().unwrap();
        let mut resolver = Resolver::new().unwrap();
        let res_vec = thread_pool.run(resolver.transform("localhost:1339".to_owned().try_into().unwrap()));

        let loopback = Ipv4Addr::new(127, 0, 0, 1);
        let socket_addr = SocketAddr::new(IpAddr::V4(loopback), 1339);
        assert_eq!(res_vec, vec![socket_addr]);
    }
}
