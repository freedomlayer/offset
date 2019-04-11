use std::{thread, time};

use tempfile::tempdir;

use bin::stindexlib::{stindex, StIndexCmd};
use bin::stnodelib::{stnode, StNodeCmd};
use bin::strelaylib::{strelay, StRelayCmd};

use stctrl::info::{FriendsCmd, InfoCmd};
use stctrl::stctrllib::{stctrl, StCtrlCmd, StCtrlSubcommand};

use crate::cli_tests::stctrl_setup::{create_stctrl_setup, StCtrlSetup};

// TODO: How do we ever close the spawned threads?
/// Spawn relay servers, index servers and nodes as threads
fn spawn_entities(stctrl_setup: &StCtrlSetup) {
    // Spawn index0:
    let st_index_cmd = StIndexCmd {
        idfile: stctrl_setup
            .temp_dir_path
            .join("index0")
            .join("index0.ident"),
        lclient: stctrl_setup.index0_client_addr.parse().unwrap(),
        lserver: stctrl_setup.index0_server_addr.parse().unwrap(),
        trusted: stctrl_setup.temp_dir_path.join("index0").join("trusted"),
    };
    // TODO: How can we close this thread?
    thread::spawn(move || stindex(st_index_cmd));

    // Spawn index1:
    let st_index_cmd = StIndexCmd {
        idfile: stctrl_setup
            .temp_dir_path
            .join("index1")
            .join("index1.ident"),
        lclient: stctrl_setup.index1_client_addr.parse().unwrap(),
        lserver: stctrl_setup.index1_server_addr.parse().unwrap(),
        trusted: stctrl_setup.temp_dir_path.join("index1").join("trusted"),
    };
    // TODO: How can we close this thread?
    thread::spawn(move || stindex(st_index_cmd));

    // Spawn relay0:
    let st_relay_cmd = StRelayCmd {
        idfile: stctrl_setup
            .temp_dir_path
            .join("relay0")
            .join("relay0.ident"),
        laddr: stctrl_setup.relay0_addr.parse().unwrap(),
    };
    // TODO: How can we close this thread?
    thread::spawn(move || strelay(st_relay_cmd));

    // Spawn relay1:
    let st_relay_cmd = StRelayCmd {
        idfile: stctrl_setup
            .temp_dir_path
            .join("relay1")
            .join("relay1.ident"),
        laddr: stctrl_setup.relay1_addr.parse().unwrap(),
    };
    // TODO: How can we close this thread?
    thread::spawn(move || strelay(st_relay_cmd));

    // Spawn node0:
    let st_node_cmd = StNodeCmd {
        idfile: stctrl_setup.temp_dir_path.join("node0").join("node0.ident"),
        laddr: stctrl_setup.node0_addr.clone().parse().unwrap(),
        database: stctrl_setup.temp_dir_path.join("node0").join("node0.db"),
        trusted: stctrl_setup.temp_dir_path.join("node0").join("trusted"),
    };
    // TODO: How can we close this thread?
    thread::spawn(move || stnode(st_node_cmd));

    // Spawn node1:
    let st_node_cmd = StNodeCmd {
        idfile: stctrl_setup.temp_dir_path.join("node1").join("node1.ident"),
        laddr: stctrl_setup.node1_addr.clone().parse().unwrap(),
        database: stctrl_setup.temp_dir_path.join("node1").join("node1.db"),
        trusted: stctrl_setup.temp_dir_path.join("node1").join("trusted"),
    };
    // TODO: How can we close this thread?
    thread::spawn(move || stnode(st_node_cmd));
}

#[test]
fn basic_invoice_receipt() {
    let _ = env_logger::init();

    // Create a temporary directory.
    // Should be deleted when gets out of scope:
    let temp_dir = tempdir().unwrap();
    let temp_dir_path = temp_dir.path().to_path_buf();
    let stctrl_setup = create_stctrl_setup(&temp_dir_path);

    spawn_entities(&stctrl_setup);

    // Wait until app0 manages to connect to node0:
    // -------------------------------------------
    // Show friends:
    let friends_cmd = FriendsCmd {};
    let info_cmd = InfoCmd::Friends(friends_cmd);
    let subcommand = StCtrlSubcommand::Info(info_cmd);

    let st_ctrl_cmd = StCtrlCmd {
        idfile: stctrl_setup.temp_dir_path.join("app0").join("app0.ident"),
        node_ticket: stctrl_setup
            .temp_dir_path
            .join("node0")
            .join("node0.ticket"),
        subcommand,
    };

    while stctrl(st_ctrl_cmd.clone(), &mut Vec::new()).is_err() {
        thread::sleep(time::Duration::from_millis(100));
    }

    // Wait until app1 manages to connect to node1:
    // -------------------------------------------
    // Show friends:
    let friends_cmd = FriendsCmd {};
    let info_cmd = InfoCmd::Friends(friends_cmd);
    let subcommand = StCtrlSubcommand::Info(info_cmd);

    let st_ctrl_cmd = StCtrlCmd {
        idfile: stctrl_setup.temp_dir_path.join("app1").join("app1.ident"),
        node_ticket: stctrl_setup
            .temp_dir_path
            .join("node1")
            .join("node1.ticket"),
        subcommand,
    };

    while stctrl(st_ctrl_cmd.clone(), &mut Vec::new()).is_err() {
        thread::sleep(time::Duration::from_millis(100));
    }
}
