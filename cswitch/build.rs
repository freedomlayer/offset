extern crate capnpc;

macro_rules! build_schema {
    ($($path: expr),*) => {
        capnpc::CompilerCommand::new()
        .src_prefix("src")
        $(.file($path))*
        .run()
        .unwrap();
    };
}

fn main() {
    build_schema!{
        "src/schema/common.capnp",
        "src/schema/indexer.capnp",
        "src/schema/channeler.capnp"
    }
}
