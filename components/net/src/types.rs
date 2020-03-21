/*
use std::net::{SocketAddr, IpAddr, Ipv4Addr, Ipv6Addr};
use proto::net::messages::{TcpAddress, TcpAddressV4, TcpAddressV6};

/// Convert offset's TcpAddress to SocketAddr
pub fn tcp_address_to_socket_addr(tcp_address: &TcpAddress) -> SocketAddr {
    match tcp_address {
        TcpAddress::V4(tcp_address_v4) => {
            let octets = &tcp_address_v4.octets;
            let ipv4_addr = Ipv4Addr::new(octets[0], octets[1], octets[2], octets[3]);
            SocketAddr::new(IpAddr::V4(ipv4_addr), tcp_address_v4.port)
        },
        TcpAddress::V6(tcp_address_v6) => {
            let segments = &tcp_address_v6.segments;
            let ipv4_addr = Ipv6Addr::new(segments[0], segments[1], segments[2], segments[3],
                                          segments[4], segments[5], segments[6], segments[7]);
            SocketAddr::new(IpAddr::V6(ipv4_addr), tcp_address_v6.port)
        },
    }
}


/// Convert SocketAddr to offset's TcpAddress
pub fn socket_addr_to_tcp_address(socket_addr: &SocketAddr) -> TcpAddress {
    let port = socket_addr.port();
    match socket_addr.ip() {
        IpAddr::V4(ipv4_addr) => {
            let tcp_address_v4 = TcpAddressV4 {
                octets: ipv4_addr.octets().clone(),
                port,
            };
            TcpAddress::V4(tcp_address_v4)
        },
        IpAddr::V6(ipv6_addr) => {
            let tcp_address_v6 = TcpAddressV6 {
                segments: ipv6_addr.segments().clone(),
                port,
            };
            TcpAddress::V6(tcp_address_v6)
        },
    }
}
*/
