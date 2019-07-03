use crate::common_capnp;
use capnp_conv::{CapnpConvError, ReadCapnp, WriteCapnp};

use super::{
    DhPublicKey, HashResult, HashedLock, InvoiceId, PaymentId, PlainLock, PrivateKey, PublicKey,
    RandValue, Salt, Signature, Uid,
};

/// Define read and write functions for basic types
macro_rules! type_capnp_serde128 {
    ($capnp_type:ident, $native_type:ident) => {
        impl<'a> WriteCapnp<'a> for $native_type {
            type WriteType = $capnp_type::Builder<'a>;

            fn write_capnp(&self, writer: &mut Self::WriterType) {
                let mut reader = io::Cursor::new(self.as_ref());
                // TODO: How to perform indexed repetitions using macro_rules!  ??
                writer.set_x0(reader.read_u64::<BigEndian>().unwrap());
                writer.set_x1(reader.read_u64::<BigEndian>().unwrap());
            }
        }

        impl<'a> ReadCapnp<'a> for $name {
            type ReadType = $capnp_type::Reader<'a>;

            fn read_capnp(reader: &Self::ReaderType) -> Result<Self, CapnpConvError> {
                let mut vec = Vec::new();
                vec.write_u64::<BigEndian>(from.get_x0())?;
                vec.write_u64::<BigEndian>(from.get_x1())?;
                Ok(Self::try_from(&vec).unwrap())
            }
        }
    };
}
