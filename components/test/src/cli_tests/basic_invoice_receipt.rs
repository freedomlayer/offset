use std::{str, thread, time};

use tempfile::tempdir;

use bin::stindexlib::{stindex, StIndexCmd};
use bin::stnodelib::{stnode, StNodeCmd};
use bin::strelaylib::{strelay, StRelayCmd};

use stctrl::config::{AddFriendCmd, AddIndexCmd, AddRelayCmd, ConfigCmd, EnableFriendCmd};
use stctrl::info::{ExportTicketCmd, FriendsCmd, InfoCmd};
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

    // Wait until apps can connect to nodes:
    for j in 0..2 {
        let friends_cmd = FriendsCmd {};
        let info_cmd = InfoCmd::Friends(friends_cmd);
        let subcommand = StCtrlSubcommand::Info(info_cmd);

        let st_ctrl_cmd = StCtrlCmd {
            idfile: stctrl_setup
                .temp_dir_path
                .join(format!("app{}", j))
                .join(format!("app{}.ident", j)),
            node_ticket: stctrl_setup
                .temp_dir_path
                .join(format!("node{}", j))
                .join(format!("node{}.ticket", j)),
            subcommand,
        };

        let mut output = Vec::new();
        while stctrl(st_ctrl_cmd.clone(), &mut output).is_err() {
            thread::sleep(time::Duration::from_millis(100));
            output.clear();
        }
        assert!(str::from_utf8(&output)
            .unwrap()
            .contains("No configured friends"));
    }

    // Configure relay servers for nodes:
    // -----------------------------

    for j in 0..2 {
        // Node0: Add relay0:
        let add_relay_cmd = AddRelayCmd {
            relay_file: stctrl_setup
                .temp_dir_path
                .join(format!("relay{}", j))
                .join(format!("relay{}.ticket", j)),
            relay_name: format!("relay{}", j),
        };
        let config_cmd = ConfigCmd::AddRelay(add_relay_cmd);
        let subcommand = StCtrlSubcommand::Config(config_cmd);

        let st_ctrl_cmd = StCtrlCmd {
            idfile: stctrl_setup
                .temp_dir_path
                .join(format!("app{}", j))
                .join(format!("app{}.ident", j)),
            node_ticket: stctrl_setup
                .temp_dir_path
                .join(format!("node{}", j))
                .join(format!("node{}.ticket", j)),
            subcommand,
        };
        stctrl(st_ctrl_cmd, &mut Vec::new()).unwrap();
    }

    // Configure index servers for nodes:
    // -----------------------------

    for j in 0..2 {
        // Node0: Add index0:
        let add_index_cmd = AddIndexCmd {
            index_file: stctrl_setup
                .temp_dir_path
                .join(format!("index{}", j))
                .join(format!("index{}_client.ticket", j)),
            index_name: format!("index{}", j),
        };
        let config_cmd = ConfigCmd::AddIndex(add_index_cmd);
        let subcommand = StCtrlSubcommand::Config(config_cmd);

        let st_ctrl_cmd = StCtrlCmd {
            idfile: stctrl_setup
                .temp_dir_path
                .join(format!("app{}", j))
                .join(format!("app{}.ident", j)),
            node_ticket: stctrl_setup
                .temp_dir_path
                .join(format!("node{}", j))
                .join(format!("node{}.ticket", j)),
            subcommand,
        };
        stctrl(st_ctrl_cmd, &mut Vec::new()).unwrap();
    }

    // Export friend ticket files:
    // ---------------------------

    for j in 0..2 {
        // Node0: Add node1 as a friend:
        let export_ticket_cmd = ExportTicketCmd {
            output_file: stctrl_setup
                .temp_dir_path
                .join(format!("app{}", j))
                .join(format!("node{}.friend", j)),
        };
        let info_cmd = InfoCmd::ExportTicket(export_ticket_cmd);
        let subcommand = StCtrlSubcommand::Info(info_cmd);

        let st_ctrl_cmd = StCtrlCmd {
            idfile: stctrl_setup
                .temp_dir_path
                .join(format!("app{}", j))
                .join(format!("app{}.ident", j)),
            node_ticket: stctrl_setup
                .temp_dir_path
                .join(format!("node{}", j))
                .join(format!("node{}.ticket", j)),
            subcommand,
        };
        stctrl(st_ctrl_cmd, &mut Vec::new()).unwrap();
    }

    // Add friends
    // ------------

    for j in 0..2 {
        // node0 has a plus of 20 credits
        let balance = if j == 0 { 20 } else { -20 };
        // Node0: Add node1 as a friend
        let add_friend_cmd = AddFriendCmd {
            friend_file: stctrl_setup
                .temp_dir_path
                .join(format!("app{}", 1 - j))
                .join(format!("node{}.friend", 1 - j)),
            friend_name: format!("node{}", 1 - j),
            balance,
        };
        let config_cmd = ConfigCmd::AddFriend(add_friend_cmd);
        let subcommand = StCtrlSubcommand::Config(config_cmd);

        let st_ctrl_cmd = StCtrlCmd {
            idfile: stctrl_setup
                .temp_dir_path
                .join(format!("app{}", j))
                .join(format!("app{}.ident", j)),
            node_ticket: stctrl_setup
                .temp_dir_path
                .join(format!("node{}", j))
                .join(format!("node{}.ticket", j)),
            subcommand,
        };
        stctrl(st_ctrl_cmd, &mut Vec::new()).unwrap();
    }

    // Enable friends
    // ---------------
    for j in 0..2 {
        let enable_friend_cmd = EnableFriendCmd {
            friend_name: format!("node{}", 1 - j),
        };
        let config_cmd = ConfigCmd::EnableFriend(enable_friend_cmd);
        let subcommand = StCtrlSubcommand::Config(config_cmd);

        let st_ctrl_cmd = StCtrlCmd {
            idfile: stctrl_setup
                .temp_dir_path
                .join(format!("app{}", j))
                .join(format!("app{}.ident", j)),
            node_ticket: stctrl_setup
                .temp_dir_path
                .join(format!("node{}", j))
                .join(format!("node{}.ticket", j)),
            subcommand,
        };
        stctrl(st_ctrl_cmd, &mut Vec::new()).unwrap();
    }

    /*
    // Wait until apps can connect to nodes:
    for j in 0..2 {
        let friends_cmd = FriendsCmd {};
        let info_cmd = InfoCmd::Friends(friends_cmd);
        let subcommand = StCtrlSubcommand::Info(info_cmd);

        let st_ctrl_cmd = StCtrlCmd {
            idfile: stctrl_setup
                .temp_dir_path
                .join(format!("app{}", j))
                .join(format!("app{}.ident", j)),
            node_ticket: stctrl_setup
                .temp_dir_path
                .join(format!("node{}", j))
                .join(format!("node{}.ticket", j)),
            subcommand,
        };

        let mut output = Vec::new();
        while stctrl(st_ctrl_cmd.clone(), &mut output).is_err() {
            thread::sleep(time::Duration::from_millis(100));
            output.clear();
        }
        assert!(str::from_utf8(&output)
            .unwrap()
            .contains("No configured friends"));
    }
    */
}
