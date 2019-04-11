use std::fs;

use bin::stmgrlib::{stmgr, GenIdentCmd, InitNodeDbCmd, StMgrCmd};
use tempfile::tempdir;

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

    // Create a temporary directory.
    // Should be deleted when gets out of scope:
    let temp_dir = tempdir().unwrap();
    let temp_dir_path = temp_dir.path().to_path_buf();

    // Prepare directories for all entities in the test:
    fs::create_dir(temp_dir_path.join("app0")).unwrap();
    fs::create_dir(temp_dir_path.join("app1")).unwrap();
    fs::create_dir(temp_dir_path.join("index0")).unwrap();
    fs::create_dir(temp_dir_path.join("index1")).unwrap();
    fs::create_dir(temp_dir_path.join("node0")).unwrap();
    fs::create_dir(temp_dir_path.join("node1")).unwrap();
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

    // Create initial databases for node0 and node1:
    for node in &["node0", "node1"] {
        let init_node_db_cmd = InitNodeDbCmd {
            idfile: temp_dir_path.join(node).join(format!("{}.ident", node)),
            output: temp_dir_path.join(node).join(format!("{}.db", node)),
        };
        let st_mgr_cmd = StMgrCmd::InitNodeDb(init_node_db_cmd);
        stmgr(st_mgr_cmd).unwrap();
    }

    /*
    let db_path_buf = self.temp_dir_path.join(format!("db_{}", index));

    let gen_ident_cmd = GenIdentCmd {
    }
    */
}
