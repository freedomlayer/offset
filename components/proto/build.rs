extern crate capnpc;

macro_rules! build_schema {
    ($($path: expr),*) => {
        capnpc::CompilerCommand::new()
        .src_prefix("src/")
        $(.file($path))*
        .run()
        .unwrap();
    };
}

fn main() {
    build_schema!{
        "src/schema/channeler.capnp",
        "src/schema/common.capnp",
        "src/schema/funder.capnp",
        "src/schema/dh.capnp",
        "src/schema/relay.capnp",
        "src/schema/keepalive.capnp"
    }
}
