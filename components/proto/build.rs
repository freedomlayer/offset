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
    build_schema! {
        "src/schema/common.capnp",
        "src/schema/funder.capnp",
        "src/schema/dh.capnp",
        "src/schema/relay.capnp",
        "src/schema/keepalive.capnp",
        "src/schema/app_server.capnp",
        "src/schema/report.capnp",
        "src/schema/index.capnp"
    }
}
