use std::{str, thread, time};

use tempfile::tempdir;

use bin::stindexlib::{stindex, StIndexCmd};
use bin::stnodelib::{stnode, StNodeCmd};
use bin::strelaylib::{strelay, StRelayCmd};

use stctrl::config::{
    AddFriendCmd, AddIndexCmd, AddRelayCmd, ConfigCmd, EnableFriendCmd, OpenFriendCmd,
    SetFriendMaxDebtCmd,
};
use stctrl::funds::{FundsCmd, PayInvoiceCmd, SendFundsCmd};
use stctrl::info::{
    BalanceCmd, ExportTicketCmd, FriendLastTokenCmd, FriendsCmd, InfoCmd, PublicKeyCmd,
};
use stctrl::stctrllib::{stctrl, StCtrlCmd, StCtrlSubcommand};

use stctrl::stregisterlib::{
    stregister, GenInvoiceCmd, StRegisterCmd, VerifyReceiptCmd, VerifyTokenCmd,
};

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

/// Get the public key of node{index}:
fn get_node_public_key(stctrl_setup: &StCtrlSetup, index: usize) -> String {
    let public_key_cmd = PublicKeyCmd {};
    let info_cmd = InfoCmd::PublicKey(public_key_cmd);
    let subcommand = StCtrlSubcommand::Info(info_cmd);

    let st_ctrl_cmd = StCtrlCmd {
        idfile: stctrl_setup
            .temp_dir_path
            .join(format!("app{}", index))
            .join(format!("app{}.ident", index)),
        node_ticket: stctrl_setup
            .temp_dir_path
            .join(format!("node{}", index))
            .join(format!("node{}.ticket", index)),
        subcommand,
    };

    let mut output = Vec::new();
    stctrl(st_ctrl_cmd.clone(), &mut output).unwrap();
    // A trick to get rid of the trailing newline:
    str::from_utf8(&output)
        .unwrap()
        .lines()
        .next()
        .unwrap()
        .to_owned()
}

/// Cnofigure mutual credits between node0 and node1
fn configure_mutual_credit(stctrl_setup: &StCtrlSetup) {
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

    // Wait until friends are seen enabled and online:
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

        // Wait until the remote friend seems to be enabled and online:
        loop {
            let mut output = Vec::new();
            stctrl(st_ctrl_cmd.clone(), &mut output).unwrap();
            let output_string = str::from_utf8(&output).unwrap();
            if output_string.contains("E+") {
                break;
            }
            thread::sleep(time::Duration::from_millis(100));
        }
    }

    // Open friends
    // ---------------
    for j in 0..2 {
        let open_friend_cmd = OpenFriendCmd {
            friend_name: format!("node{}", 1 - j),
        };
        let config_cmd = ConfigCmd::OpenFriend(open_friend_cmd);
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

    // Wait until requests are seen open:
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

        // Wait until the local requests and remote requests are both open:
        loop {
            let mut output = Vec::new();
            stctrl(st_ctrl_cmd.clone(), &mut output).unwrap();
            let output_string = str::from_utf8(&output).unwrap();
            if output_string.contains("LR=+") && output_string.contains("RR=+") {
                break;
            }
            thread::sleep(time::Duration::from_millis(100));
        }
    }
}

/// Set max_debt for node1, and then send funds from node1 to node0
fn send_funds(stctrl_setup: &StCtrlSetup) {
    // node0 sets remote max debt for node1:
    let set_friend_max_debt_cmd = SetFriendMaxDebtCmd {
        friend_name: "node1".to_owned(),
        max_debt: 200,
    };
    let config_cmd = ConfigCmd::SetFriendMaxDebt(set_friend_max_debt_cmd);
    let subcommand = StCtrlSubcommand::Config(config_cmd);

    let st_ctrl_cmd = StCtrlCmd {
        idfile: stctrl_setup.temp_dir_path.join("app0").join("app0.ident"),
        node_ticket: stctrl_setup
            .temp_dir_path
            .join("node0")
            .join("node0.ticket"),
        subcommand,
    };
    stctrl(st_ctrl_cmd, &mut Vec::new()).unwrap();

    // Wait until node1 sees that his local max debt is 200:
    // -----------------------------------------------------
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

    loop {
        let mut output = Vec::new();
        stctrl(st_ctrl_cmd.clone(), &mut output).unwrap();
        let output_string = str::from_utf8(&output).unwrap();
        if output_string.contains("LMD=200") {
            break;
        }
        thread::sleep(time::Duration::from_millis(100));
    }

    // Get the public key of node0:
    let node0_pk_string = get_node_public_key(stctrl_setup, 0);

    // node1 sends credits to node0:
    // -----------------------------
    let send_funds_cmd = SendFundsCmd {
        destination_str: node0_pk_string,
        dest_payment: 50,
        opt_receipt_file: Some(
            stctrl_setup
                .temp_dir_path
                .join("app1")
                .join("receipt_50.receipt"),
        ),
    };
    let funds_cmd = FundsCmd::SendFunds(send_funds_cmd);
    let subcommand = StCtrlSubcommand::Funds(funds_cmd);

    let st_ctrl_cmd = StCtrlCmd {
        idfile: stctrl_setup.temp_dir_path.join("app1").join("app1.ident"),
        node_ticket: stctrl_setup
            .temp_dir_path
            .join("node1")
            .join("node1.ticket"),
        subcommand,
    };
    // Attempt to pay. We might need to wait a bit first until the route is registered with the
    // index servers:
    let mut output = Vec::new();
    loop {
        if stctrl(st_ctrl_cmd.clone(), &mut output).is_ok() {
            break;
        }
        thread::sleep(time::Duration::from_millis(100));
        output.clear();
    }

    // node1's balance should now be -70
    // -----------------------------------
    // Note: -70 = -20 (initial) - 50 (last payment):
    //
    let balance_cmd = BalanceCmd {};
    let info_cmd = InfoCmd::Balance(balance_cmd);
    let subcommand = StCtrlSubcommand::Info(info_cmd);

    let st_ctrl_cmd = StCtrlCmd {
        idfile: stctrl_setup.temp_dir_path.join("app1").join("app1.ident"),
        node_ticket: stctrl_setup
            .temp_dir_path
            .join("node1")
            .join("node1.ticket"),
        subcommand,
    };

    let mut output = Vec::new();
    stctrl(st_ctrl_cmd.clone(), &mut output).unwrap();
    assert!(str::from_utf8(&output).unwrap().contains("-70"));
}

/// Node0: generate an invoice
/// Node1: pay the invoice
/// Node0: verify the receipt
fn pay_invoice(stctrl_setup: &StCtrlSetup) {
    // Get the public key of node0:
    let node0_pk_string = get_node_public_key(stctrl_setup, 0);

    // Node0: generate an invoice:
    // ---------------------------
    let gen_invoice_cmd = GenInvoiceCmd {
        public_key: node0_pk_string,
        amount: 40,
        output: stctrl_setup
            .temp_dir_path
            .join("node0")
            .join("node0_40.invoice"),
    };

    let stregister_cmd = StRegisterCmd::GenInvoice(gen_invoice_cmd);
    stregister(stregister_cmd, &mut Vec::new()).unwrap();

    // Node1: pay the invoice:
    // -----------------------
    let pay_invoice_cmd = PayInvoiceCmd {
        invoice_file: stctrl_setup
            .temp_dir_path
            .join("node0")
            .join("node0_40.invoice"),
        receipt_file: stctrl_setup
            .temp_dir_path
            .join("node1")
            .join("receipt_40.receipt"),
    };
    let funds_cmd = FundsCmd::PayInvoice(pay_invoice_cmd);
    let subcommand = StCtrlSubcommand::Funds(funds_cmd);

    let st_ctrl_cmd = StCtrlCmd {
        idfile: stctrl_setup.temp_dir_path.join("app1").join("app1.ident"),
        node_ticket: stctrl_setup
            .temp_dir_path
            .join("node1")
            .join("node1.ticket"),
        subcommand,
    };
    stctrl(st_ctrl_cmd.clone(), &mut Vec::new()).unwrap();

    // Verify the receipt:
    // ------------------
    let verify_receipt_cmd = VerifyReceiptCmd {
        invoice: stctrl_setup
            .temp_dir_path
            .join("node0")
            .join("node0_40.invoice"),
        receipt: stctrl_setup
            .temp_dir_path
            .join("node1")
            .join("receipt_40.receipt"),
    };

    let stregister_cmd = StRegisterCmd::VerifyReceipt(verify_receipt_cmd);
    let mut output = Vec::new();
    stregister(stregister_cmd, &mut output).unwrap();
    assert!(str::from_utf8(&output).unwrap().contains("is valid!"));
}

/// Export a friend's last token and then verify it
fn export_token(stctrl_setup: &StCtrlSetup) {
    // node0: Get node1's last token:
    let friend_last_token_cmd = FriendLastTokenCmd {
        friend_name: "node1".to_owned(),
        output_file: stctrl_setup.temp_dir_path.join("node0").join("node1.token"),
    };
    let info_cmd = InfoCmd::FriendLastToken(friend_last_token_cmd);
    let subcommand = StCtrlSubcommand::Info(info_cmd);

    let st_ctrl_cmd = StCtrlCmd {
        idfile: stctrl_setup.temp_dir_path.join("app0").join("app0.ident"),
        node_ticket: stctrl_setup
            .temp_dir_path
            .join("node0")
            .join("node0.ticket"),
        subcommand,
    };
    stctrl(st_ctrl_cmd, &mut Vec::new()).unwrap();

    // Verify the token:
    // ------------------
    let verify_token_cmd = VerifyTokenCmd {
        token: stctrl_setup.temp_dir_path.join("node0").join("node1.token"),
    };

    let stregister_cmd = StRegisterCmd::VerifyToken(verify_token_cmd);
    let mut output = Vec::new();
    stregister(stregister_cmd, &mut output).unwrap();
    let output_str = str::from_utf8(&output).unwrap();
    assert!(output_str.contains("is valid!"));
    assert!(output_str.contains("balance: -70"));
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
    configure_mutual_credit(&stctrl_setup);
    send_funds(&stctrl_setup);
    pay_invoice(&stctrl_setup);
    export_token(&stctrl_setup);
}
