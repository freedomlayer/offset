extern crate capnpc;

fn main() {  
    // println!("Hello world!");
    ::capnpc::CompilerCommand::new()
        .src_prefix("src")
        .file("src/schema/channeler.capnp")
        .run()
        .unwrap();
}
