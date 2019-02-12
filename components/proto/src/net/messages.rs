// use byteorder::{WriteBytesExt, BigEndian};
use common::canonical_serialize::CanonicalSerialize;

/*
/// IPv4 address (TCP)
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TcpAddressV4 {
    pub octets: [u8; 4], // 32 bit
    pub port: u16,
}

/// IPv6 address (TCP)
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TcpAddressV6 {
    pub segments: [u16; 8], // 128 bit
    pub port: u16,
}

// TODO: Possibly move TcpAddress and the structs it depends on 
// to a more generic module in proto?
/// Address for TCP connection
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TcpAddress {
    V4(TcpAddressV4),
    V6(TcpAddressV6),
}
*/

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct NetAddress(pub String);

impl CanonicalSerialize for NetAddress {
    fn canonical_serialize(&self) -> Vec<u8> {
        self.0.canonical_serialize()
    }
}

impl From<String> for NetAddress {
    fn from(address: String) -> NetAddress {
        NetAddress(address)
    }
}

/*

impl CanonicalSerialize for TcpAddressV4 {
    fn canonical_serialize(&self) -> Vec<u8> {
        let mut res_bytes = Vec::new();
        res_bytes.extend_from_slice(&self.octets);
        res_bytes.write_u16::<BigEndian>(self.port).unwrap();
        res_bytes
    }
}

impl CanonicalSerialize for TcpAddressV6 {
    fn canonical_serialize(&self) -> Vec<u8> {
        let mut res_bytes = Vec::new();
        for s in self.segments.iter() {
            res_bytes.push((s >> 8) as u8);
            res_bytes.push((s & 0xff) as u8);
        };
        res_bytes.write_u16::<BigEndian>(self.port).unwrap();
        res_bytes
    }
}

impl CanonicalSerialize for TcpAddress {
    fn canonical_serialize(&self) -> Vec<u8> {
        let mut res_bytes = Vec::new();
        match self {
            TcpAddress::V4(tcp_address_v4) => {
                res_bytes.push(0);
                res_bytes.extend_from_slice(&tcp_address_v4.canonical_serialize());
            },
            TcpAddress::V6(tcp_address_v4) => {
                res_bytes.push(1);
                res_bytes.extend_from_slice(&tcp_address_v4.canonical_serialize());
            },
        }
        res_bytes
    }
}
*/

