use byteorder::{ByteOrder, ReadBytesExt, WriteBytesExt};
use bytes::BigEndian;

use serde::{Deserialize, Serialize};

use capnp_conv::{CapnpConvError, ReadCapnp, WriteCapnp};

/*
#[derive(derive_more::From, derive_more::New, Debug)]
struct CustomU128(u128);

#[derive(derive_more::From, derive_more::New, Debug)]
struct CustomI128(i128);
*/

#[derive(
    derive_more::Constructor,
    derive_more::From,
    Serialize,
    Deserialize,
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
)]
pub struct Wrapper<T>(T);

impl<T> std::ops::Deref for Wrapper<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<'a> WriteCapnp<'a> for Wrapper<u128> {
    type WriterType = crate::common_capnp::custom_u_int128::Builder<'a>;

    fn write_capnp(&self, writer: &mut Self::WriterType) {
        let mut inner = writer.reborrow().get_inner().unwrap();

        let mut data_bytes = Vec::new();
        data_bytes.write_u128::<BigEndian>(**self).unwrap();
        let mut cursor = std::io::Cursor::new(AsRef::<[u8]>::as_ref(&data_bytes));

        inner.set_x0(cursor.read_u64::<BigEndian>().unwrap());
        inner.set_x1(cursor.read_u64::<BigEndian>().unwrap());
    }
}

impl<'a> ReadCapnp<'a> for Wrapper<u128> {
    type ReaderType = crate::common_capnp::custom_u_int128::Reader<'a>;

    fn read_capnp(reader: &Self::ReaderType) -> Result<Self, CapnpConvError> {
        let inner = reader.get_inner()?;
        let mut vec = Vec::new();
        vec.write_u64::<BigEndian>(inner.get_x0())?;
        vec.write_u64::<BigEndian>(inner.get_x1())?;
        Ok(Wrapper::new(BigEndian::read_u128(&vec[..])))
    }
}

impl<'a> WriteCapnp<'a> for Wrapper<i128> {
    type WriterType = crate::common_capnp::custom_int128::Builder<'a>;

    fn write_capnp(&self, writer: &mut Self::WriterType) {
        let mut inner = writer.reborrow().get_inner().unwrap();

        let mut data_bytes = Vec::new();
        data_bytes.write_i128::<BigEndian>(**self).unwrap();
        let mut cursor = std::io::Cursor::new(AsRef::<[u8]>::as_ref(&data_bytes));

        inner.set_x0(cursor.read_u64::<BigEndian>().unwrap());
        inner.set_x1(cursor.read_u64::<BigEndian>().unwrap());
    }
}

impl<'a> ReadCapnp<'a> for Wrapper<i128> {
    type ReaderType = crate::common_capnp::custom_int128::Reader<'a>;

    fn read_capnp(reader: &Self::ReaderType) -> Result<Self, CapnpConvError> {
        let inner = reader.get_inner()?;
        let mut vec = Vec::new();
        vec.write_u64::<BigEndian>(inner.get_x0())?;
        vec.write_u64::<BigEndian>(inner.get_x1())?;
        Ok(Wrapper::new(BigEndian::read_i128(&vec[..])))
    }
}
