use std::fs;

use bin::stmgrlib::{stmgr, GenIdentCmd, InitNodeDbCmd, StMgrCmd};
use tempfile::tempdir;

use std::net::{IpAddr, Ipv4Addr, SocketAddr, TcpListener};

/// Get a list of `num_ports` available TCP port we can listen on
fn get_available_ports(num_ports: usize) -> Vec<u16> {
    // Idea based on code at:
    // https://github.com/rust-lang-nursery/rust-cookbook/pull/137/files

    // It's important that we collect listeners in a vector and not drop them immediately, so that
    // we get a list of distinct ports, and not the same port `num_ports` times.
    let mut listeners = Vec::new();
    for _ in 0..num_ports {
        let loopback = Ipv4Addr::new(127, 0, 0, 1);
        // Assigning port 0 requests the OS to assign a free port
        let socket_addr = SocketAddr::new(IpAddr::V4(loopback), 0);
        listeners.push(TcpListener::bind(&socket_addr).unwrap());
    }

    listeners
        .into_iter()
        .map(|listener| listener.local_addr().unwrap().port())
        .collect()
}

#[test]
fn basic_offst_setup() {
    // TODO:
    /*
    ├── app0
    │   ├── app0.ident
    │   ├── app0.ticket
    │   └── node0.friend
    ├── app1
    │   ├── app1.ident
    │   ├── app1.ticket
    │   └── node1.friend
    ├── index_server0
    │   ├── index0_client.ticket
    │   ├── index0_server.ticket
    │   ├── index_server0.ident
    │   └── trusted
    │       └── index_ticket1
    ├── index_server1
    │   ├── index1_client.ticket
    │   ├── index1_server.ticket
    │   ├── index_server1.ident
    │   └── trusted
    │       └── index_ticket0
    ├── node0
    │   ├── node0.db
    │   ├── node0.ident
    │   ├── node0.ticket
    │   └── trusted
    │       └── app0.ticket
    ├── node1
    │   ├── node1.db
    │   ├── node1.ident
    │   ├── node1.ticket
    │   └── trusted
    │       └── app1.ticket
    ├── relay0
    │   ├── relay0.ident
    │   └── relay0.ticket
    └── relay1
        ├── relay1.ident
        └── relay1.ticket
    */

    // Assign listening addresses for all services:
    let ports = dbg!(get_available_ports(6));
    let _node0_addr = format!("localhost:{}", ports[0]);
    let _node1_addr = format!("localhost:{}", ports[1]);
    let _index0_addr = format!("localhost:{}", ports[2]);
    let _index1_addr = format!("localhost:{}", ports[3]);
    let _relay0_addr = format!("localhost:{}", ports[4]);
    let _relay1_addr = format!("localhost:{}", ports[5]);

    // Create a temporary directory.
    // Should be deleted when gets out of scope:
    let temp_dir = tempdir().unwrap();
    let temp_dir_path = temp_dir.path().to_path_buf();

    // Prepare directories for all entities in the test:
    fs::create_dir(temp_dir_path.join("app0")).unwrap();
    fs::create_dir(temp_dir_path.join("app1")).unwrap();
    fs::create_dir(temp_dir_path.join("index0")).unwrap();
    fs::create_dir(temp_dir_path.join("index0").join("trusted")).unwrap();
    fs::create_dir(temp_dir_path.join("index1")).unwrap();
    fs::create_dir(temp_dir_path.join("index1").join("trusted")).unwrap();
    fs::create_dir(temp_dir_path.join("node0")).unwrap();
    fs::create_dir(temp_dir_path.join("node0").join("trusted")).unwrap();
    fs::create_dir(temp_dir_path.join("node1")).unwrap();
    fs::create_dir(temp_dir_path.join("node1").join("trusted")).unwrap();
    fs::create_dir(temp_dir_path.join("relay0")).unwrap();
    fs::create_dir(temp_dir_path.join("relay1")).unwrap();

    // Create identities for all entities:
    for entity in &[
        "app0", "app1", "index0", "index1", "node0", "node1", "relay0", "relay1",
    ] {
        let gen_ident_cmd = GenIdentCmd {
            output: temp_dir_path.join(entity).join(format!("{}.ident", entity)),
        };
        let st_mgr_cmd = StMgrCmd::GenIdent(gen_ident_cmd);
        stmgr(st_mgr_cmd).unwrap();
    }

    // Prepare files for nodes:
    for node in &["node0", "node1"] {
        // Create initial database:
        let init_node_db_cmd = InitNodeDbCmd {
            idfile: temp_dir_path.join(node).join(format!("{}.ident", node)),
            output: temp_dir_path.join(node).join(format!("{}.db", node)),
        };
        let st_mgr_cmd = StMgrCmd::InitNodeDb(init_node_db_cmd);
        stmgr(st_mgr_cmd).unwrap();

        /*
        // Create node ticket:
        let node_ticket_cmd = NodeTicketCmd {
            idfile: temp_dir_path.join(node).join(format!("{}.ident", node)),
            output: temp_dir_path.join(node).join(format!("{}.ticket", node)),
            address: "localhost:
        };
        let st_mgr_cmd = StMgrCmd::InitNodeDb(init_node_db_cmd);
        stmgr(st_mgr_cmd).unwrap();
        */
        
    }

    /*
    let db_path_buf = self.temp_dir_path.join(format!("db_{}", index));

    let gen_ident_cmd = GenIdentCmd {
    }
    */
}
