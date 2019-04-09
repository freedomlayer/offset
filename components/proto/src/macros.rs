/// Include a capnp schema
macro_rules! include_schema {
    ($( $name:ident, $path:expr );*) => {
        $(
            #[allow(unused, clippy::all)]
            pub mod $name {
                include!(concat!(env!("OUT_DIR"), "/schema/", $path, ".rs"));
            }

            // use self::$name::*;
        )*
    };
}
