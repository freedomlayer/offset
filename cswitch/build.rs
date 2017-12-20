extern crate capnpc;

fn main() {
    capnpc::CompilerCommand::new()
        .src_prefix("src")
        .file("src/schema/channeler.capnp")
        .run()
        .unwrap();

    capnpc::CompilerCommand::new()
        .src_prefix("src")
        .file("src/schema/indexer.capnp")
        .run()
        .unwrap();
}
