/// Define read and write functions for basic types
macro_rules! type_capnp_serde {
    ($native_type:ty, $capnp_type:ty, ($($field:ident),*)) => {
        paste::item! {
            impl<'a> WriteCapnp<'a> for $native_type {
                type WriterType = $capnp_type::Builder<'a>;

                fn write_capnp(&self, writer: &mut Self::WriterType) {
                    let mut inner = writer.reborrow().get_inner().unwrap();
                    let mut cursor = std::io::Cursor::new(self.as_ref());
                    $(
                        inner.[<set_ $field>](cursor.read_u64::<BigEndian>().unwrap());
                    )*
                }
            }
        }

        paste::item! {
            impl<'a> ReadCapnp<'a> for $native_type {
                type ReaderType = $capnp_type::Reader<'a>;

                fn read_capnp(reader: &Self::ReaderType) -> Result<Self, CapnpConvError> {
                    let inner = reader.get_inner()?;
                    let mut vec = Vec::new();
                    $(
                        vec.write_u64::<BigEndian>(inner.[<get_ $field>]())?;
                    )*
                    Ok(Self::try_from(&vec[..]).unwrap())
                }
            }
        }
    };
}
