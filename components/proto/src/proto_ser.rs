use derive_more::From;

use capnp_conv::{CapnpConvError, FromCapnpBytes, ToCapnpBytes};

#[derive(Debug, From)]
pub enum ProtoSerializeError {
    CapnpConvError(CapnpConvError),
}

pub trait ProtoSerialize {
    /// Serialize a Rust struct into bytes using Capnp
    fn proto_serialize(&self) -> Vec<u8>;
}

pub trait ProtoDeserialize: Sized {
    /// Deserialize a Rust struct from bytes using Capnp
    fn proto_deserialize(bytes: &[u8]) -> Result<Self, ProtoSerializeError>;
}

impl<T> ProtoSerialize for T
where
    T: ToCapnpBytes,
{
    fn proto_serialize(&self) -> Vec<u8> {
        self.to_capnp_bytes()
    }
}

impl<T> ProtoDeserialize for T
where
    T: FromCapnpBytes,
{
    fn proto_deserialize(bytes: &[u8]) -> Result<Self, ProtoSerializeError> {
        Ok(Self::from_capnp_bytes(bytes)?)
    }
}
