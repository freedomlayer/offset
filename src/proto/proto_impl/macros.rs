/// Include the auto-generated schema files.
//#[macro_export]
macro_rules! include_schema {
    ($( $name:ident, $path:expr );*) => {
        $(
            // Allow clippy's `Warn` lint group
            #[allow(unused, clippy)]
            pub mod $name {
                include!(concat!(env!("OUT_DIR"), "/schema/", $path, ".rs"));
            }

            use self::$name::*;
        )*
    };
}

/// Macro to inject the default implementation of `Proto::decode` and `Proto::encode`.
//#[macro_export]
macro_rules! inject_default_impl {
    () => {
        fn decode<B: AsRef<[u8]>>(buffer: B) -> Result<Self, ProtoError> {
            let mut buffer = io::Cursor::new(buffer);

            let reader = serialize_packed::read_message(
                &mut buffer,
                ::capnp::message::ReaderOptions::new()
            )?;

            Self::read(&reader.get_root()?)
        }

        fn encode(&self) -> Result<Bytes, ProtoError> {
            let mut builder = ::capnp::message::Builder::new_default();

            match self.write(&mut builder.init_root())? {
                () => {
                    let mut serialized_msg = Vec::new();

                    serialize_packed::write_message(
                        &mut serialized_msg,
                        &builder
                    )?;

                    Ok(Bytes::from(serialized_msg))
                }
            }
        }
    };
}

#[cfg(test)]
macro_rules! test_encode_decode {
    ($type: ident, $in: ident) => {
        let msg = $in.encode().unwrap();
        let out = $type::decode(msg).unwrap();
        assert!($in == out);
    };
}
