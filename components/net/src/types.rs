use std::net::{SocketAddr, IpAddr, Ipv4Addr, Ipv6Addr};
use proto::funder::messages::TcpAddress;

/// Convert offst's TcpAddress to SocketAddr
pub fn tcp_address_to_socket_addr(tcp_address: &TcpAddress) -> SocketAddr {
    match tcp_address {
        TcpAddress::V4(tcp_address_v4) => {
            let address = &tcp_address_v4.address;
            let ipv4_addr = Ipv4Addr::new(address[0], address[1], address[2], address[3]);
            SocketAddr::new(IpAddr::V4(ipv4_addr), tcp_address_v4.port)
        },
        TcpAddress::V6(tcp_address_v6) => {
            let address = &tcp_address_v6.address;
            // TODO: A more elegant way to write this? :
            let ipv4_addr = Ipv6Addr::new(((address[ 0] as u16) << 8) + (address[ 1] as u16), 
                                          ((address[ 2] as u16) << 8) + (address[ 3] as u16),
                                          ((address[ 4] as u16) << 8) + (address[ 5] as u16),
                                          ((address[ 6] as u16) << 8) + (address[ 7] as u16),
                                          ((address[ 8] as u16) << 8) + (address[ 9] as u16),
                                          ((address[10] as u16) << 8) + (address[11] as u16),
                                          ((address[12] as u16) << 8) + (address[13] as u16),
                                          ((address[14] as u16) << 8) + (address[15] as u16));
            SocketAddr::new(IpAddr::V6(ipv4_addr), tcp_address_v6.port)
        },
    }
}
