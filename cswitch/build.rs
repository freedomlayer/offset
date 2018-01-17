extern crate capnpc;

macro_rules! build_schema {
    ($($path: expr),*) => {
        capnpc::CompilerCommand::new()
        .src_prefix("src/proto/")
        $(.file($path))*
        .run()
        .unwrap();
    };
}

fn main() {
    build_schema!{
        "src/proto/schema/channeler.capnp",
        "src/proto/schema/common.capnp",
        "src/proto/schema/funder.capnp",
        "src/proto/schema/indexer.capnp",
        "src/proto/schema/networker.capnp"
    }
}
