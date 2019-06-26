use std::fs;
use std::path::{Path, PathBuf};

use bin::stmgrlib::{
    stmgr, AppTicketCmd, GenIdentCmd, IndexTicketCmd, InitNodeDbCmd, NodeTicketCmd, RelayTicketCmd,
    StMgrCmd,
};
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

#[derive(Debug)]
pub struct StCtrlSetup {
    pub node0_addr: String,
    pub node1_addr: String,
    pub index0_client_addr: String,
    pub index0_server_addr: String,
    pub index1_client_addr: String,
    pub index1_server_addr: String,
    pub relay0_addr: String,
    pub relay1_addr: String,
    pub temp_dir_path: PathBuf,
}

pub fn create_stctrl_setup(temp_dir_path: &Path) -> StCtrlSetup {
    /*
    ├── app0
    │   ├── app0.ident
    │   └── node0.friend
    ├── app1
    │   ├── app1.ident
    │   └── node1.friend
    ├── index0
    │   ├── index0_client.ticket
    │   ├── index0.ident
    │   └── trusted
    │       └── index1_server.ticket
    ├── index1
    │   ├── index1_client.ticket
    │   ├── index1.ident
    │   └── trusted
    │       └── index0_server.ticket
    ├── node0
    │   ├── node0.db
    │   ├── node0.ident
    │   ├── node0.ticket
    │   └── trusted
    │       └── app0.ticket
    ├── node1
    │   ├── node1.db
    │   ├── node1.ident
    │   ├── node1.ticket
    │   └── trusted
    │       └── app1.ticket
    ├── relay0
    │   ├── relay0.ident
    │   └── relay0.ticket
    └── relay1
        ├── relay1.ident
        └── relay1.ticket
    */

    // Assign listening addresses for all services:
    let ports = get_available_ports(8);
    // TODO: Is there a more generic way to express localhost than 127.0.0.1?
    // We can't use "localhost" because it requires resolving.
    let node0_addr = format!("127.0.0.1:{}", ports[0]);
    let node1_addr = format!("127.0.0.1:{}", ports[1]);
    let index0_client_addr = format!("127.0.0.1:{}", ports[2]);
    let index0_server_addr = format!("127.0.0.1:{}", ports[3]);
    let index1_client_addr = format!("127.0.0.1:{}", ports[4]);
    let index1_server_addr = format!("127.0.0.1:{}", ports[5]);
    let relay0_addr = format!("127.0.0.1:{}", ports[6]);
    let relay1_addr = format!("127.0.0.1:{}", ports[7]);

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
            output_path: temp_dir_path.join(entity).join(format!("{}.ident", entity)),
        };
        stmgr(StMgrCmd::GenIdent(gen_ident_cmd)).unwrap();
    }

    // Prepare files for nodes:
    for node in &["node0", "node1"] {
        // Create initial database:
        let init_node_db_cmd = InitNodeDbCmd {
            idfile_path: temp_dir_path.join(node).join(format!("{}.ident", node)),
            output_path: temp_dir_path.join(node).join(format!("{}.db", node)),
        };
        stmgr(StMgrCmd::InitNodeDb(init_node_db_cmd)).unwrap();
    }

    // Create node tickets:
    // --------------------
    // Create node0 ticket:
    let node_ticket_cmd = NodeTicketCmd {
        idfile_path: temp_dir_path.join("node0").join("node0.ident"),
        output_path: temp_dir_path.join("node0").join("node0.ticket"),
        address: node0_addr.clone(),
    };
    stmgr(StMgrCmd::NodeTicket(node_ticket_cmd)).unwrap();

    // Create node1 ticket:
    let node_ticket_cmd = NodeTicketCmd {
        idfile_path: temp_dir_path.join("node1").join("node1.ident"),
        output_path: temp_dir_path.join("node1").join("node1.ticket"),
        address: node1_addr.clone(),
    };
    stmgr(StMgrCmd::NodeTicket(node_ticket_cmd)).unwrap();

    // Create relay tickets:
    let relay_ticket_cmd = RelayTicketCmd {
        idfile_path: temp_dir_path.join("relay0").join("relay0.ident"),
        output_path: temp_dir_path.join("relay0").join("relay0.ticket"),
        address: relay0_addr.clone(),
    };
    stmgr(StMgrCmd::RelayTicket(relay_ticket_cmd)).unwrap();

    let relay_ticket_cmd = RelayTicketCmd {
        idfile_path: temp_dir_path.join("relay1").join("relay1.ident"),
        output_path: temp_dir_path.join("relay1").join("relay1.ticket"),
        address: relay1_addr.clone(),
    };
    stmgr(StMgrCmd::RelayTicket(relay_ticket_cmd)).unwrap();

    // Create index tickets:
    // --------------------
    let index_ticket_cmd = IndexTicketCmd {
        idfile_path: temp_dir_path.join("index0").join("index0.ident"),
        output_path: temp_dir_path.join("index0").join("index0_client.ticket"),
        address: index0_client_addr.clone(),
    };
    stmgr(StMgrCmd::IndexTicket(index_ticket_cmd)).unwrap();

    let index_ticket_cmd = IndexTicketCmd {
        idfile_path: temp_dir_path.join("index0").join("index0.ident"),
        output_path: temp_dir_path
            .join("index1")
            .join("trusted")
            .join("index0_server.ticket"),
        address: index0_server_addr.clone(),
    };
    stmgr(StMgrCmd::IndexTicket(index_ticket_cmd)).unwrap();

    let index_ticket_cmd = IndexTicketCmd {
        idfile_path: temp_dir_path.join("index1").join("index1.ident"),
        output_path: temp_dir_path.join("index1").join("index1_client.ticket"),
        address: index1_client_addr.clone(),
    };
    stmgr(StMgrCmd::IndexTicket(index_ticket_cmd)).unwrap();

    let index_ticket_cmd = IndexTicketCmd {
        idfile_path: temp_dir_path.join("index1").join("index1.ident"),
        output_path: temp_dir_path
            .join("index0")
            .join("trusted")
            .join("index1_server.ticket"),
        address: index1_server_addr.clone(),
    };
    stmgr(StMgrCmd::IndexTicket(index_ticket_cmd)).unwrap();

    // Create app tickets and store them at the corresponding nodes' trusted directory.
    // -------------------------------------------------------------------------------

    let app_ticket_cmd = AppTicketCmd {
        idfile_path: temp_dir_path.join("app0").join("app0.ident"),
        output_path: temp_dir_path
            .join("node0")
            .join("trusted")
            .join("app0.ticket"),
        proutes: true,
        pbuyer: true,
        pseller: true,
        pconfig: true,
    };
    stmgr(StMgrCmd::AppTicket(app_ticket_cmd)).unwrap();

    let app_ticket_cmd = AppTicketCmd {
        idfile_path: temp_dir_path.join("app1").join("app1.ident"),
        output_path: temp_dir_path
            .join("node1")
            .join("trusted")
            .join("app1.ticket"),
        proutes: true,
        pbuyer: true,
        pseller: true,
        pconfig: true,
    };
    stmgr(StMgrCmd::AppTicket(app_ticket_cmd)).unwrap();

    StCtrlSetup {
        node0_addr,
        node1_addr,
        index0_client_addr,
        index0_server_addr,
        index1_client_addr,
        index1_server_addr,
        relay0_addr,
        relay1_addr,
        temp_dir_path: temp_dir_path.to_path_buf(),
    }
}

#[test]
fn test_stctrl_setup() {
    // Create a temporary directory.
    // Should be deleted when gets out of scope:
    let temp_dir = tempdir().unwrap();
    let temp_dir_path = temp_dir.path().to_path_buf();
    let _stctrl_setup = create_stctrl_setup(&temp_dir_path);
}
