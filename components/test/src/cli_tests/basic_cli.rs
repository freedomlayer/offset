use std::{str, thread, time};

use tempfile::tempdir;

use bin::stindexlib::{stindex, StIndexCmd};
use bin::stnodelib::{stnode, StNodeCmd};
use bin::strelaylib::{strelay, StRelayCmd};

use stctrl::config::{
    AddFriendCmd, AddIndexCmd, AddRelayCmd, CloseFriendCurrencyCmd, ConfigCmd, DisableFriendCmd,
    EnableFriendCmd, OpenFriendCurrencyCmd, RemoveFriendCurrencyCmd, SetFriendCurrencyMaxDebtCmd,
    SetFriendCurrencyRateCmd,
};

use stctrl::buyer::{BuyerCmd, BuyerError, PayInvoiceCmd, PaymentStatusCmd};
use stctrl::info::{ExportTicketCmd, FriendLastTokenCmd, FriendsCmd, InfoCmd};
use stctrl::seller::{CancelInvoiceCmd, CommitInvoiceCmd, CreateInvoiceCmd, SellerCmd};
use stctrl::stctrllib::{stctrl, StCtrlCmd, StCtrlError, StCtrlSubcommand};
use stctrl::stverifylib::{stverify, StVerifyCmd, VerifyReceiptCmd, VerifyTokenCmd};

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
    thread::spawn(move || {
        let res = stindex(st_index_cmd);
        error!("index0 exited with: {:?}", res);
    });

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
    thread::spawn(move || {
        let res = stindex(st_index_cmd);
        error!("index1 exited with: {:?}", res);
    });

    // Spawn relay0:
    let st_relay_cmd = StRelayCmd {
        idfile: stctrl_setup
            .temp_dir_path
            .join("relay0")
            .join("relay0.ident"),
        laddr: stctrl_setup.relay0_addr.parse().unwrap(),
    };
    // TODO: How can we close this thread?
    thread::spawn(move || {
        let res = strelay(st_relay_cmd);
        error!("relay0 exited with: {:?}", res);
    });

    // Spawn relay1:
    let st_relay_cmd = StRelayCmd {
        idfile: stctrl_setup
            .temp_dir_path
            .join("relay1")
            .join("relay1.ident"),
        laddr: stctrl_setup.relay1_addr.parse().unwrap(),
    };
    // TODO: How can we close this thread?
    thread::spawn(move || {
        let res = strelay(st_relay_cmd);
        error!("relay1 exited with: {:?}", res);
    });

    // Spawn node0:
    let st_node_cmd = StNodeCmd {
        idfile: stctrl_setup.temp_dir_path.join("node0").join("node0.ident"),
        laddr: stctrl_setup.node0_addr.clone().parse().unwrap(),
        database: stctrl_setup.temp_dir_path.join("node0").join("node0.db"),
        trusted: stctrl_setup.temp_dir_path.join("node0").join("trusted"),
    };
    // TODO: How can we close this thread?
    thread::spawn(move || {
        let res = stnode(st_node_cmd);
        error!("node0 exited with: {:?}", res);
    });

    // Spawn node1:
    let st_node_cmd = StNodeCmd {
        idfile: stctrl_setup.temp_dir_path.join("node1").join("node1.ident"),
        laddr: stctrl_setup.node1_addr.clone().parse().unwrap(),
        database: stctrl_setup.temp_dir_path.join("node1").join("node1.db"),
        trusted: stctrl_setup.temp_dir_path.join("node1").join("trusted"),
    };
    // TODO: How can we close this thread?
    thread::spawn(move || {
        let res = stnode(st_node_cmd);
        error!("node1 exited with: {:?}", res);
    });
}

/*
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
*/

/// Configure mutual credits between node0 and node1
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
            relay_path: stctrl_setup
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
            index_path: stctrl_setup
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
            ticket_path: stctrl_setup
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
        // Node0: Add node1 as a friend
        let add_friend_cmd = AddFriendCmd {
            friend_path: stctrl_setup
                .temp_dir_path
                .join(format!("app{}", 1 - j))
                .join(format!("node{}.friend", 1 - j)),
            friend_name: format!("node{}", 1 - j),
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

    // Set rate
    // Note: Might not be useful in the case of only a pair of nodes.
    // Can only be tested in the case of a chain of at least 3 nodes.
    // ---------
    for j in 0..2 {
        let set_friend_currency_rate_cmd = SetFriendCurrencyRateCmd {
            friend_name: format!("node{}", 1 - j),
            currency_name: "FST".to_owned(),
            mul: 0,
            add: 1,
        };
        let config_cmd = ConfigCmd::SetFriendCurrencyRate(set_friend_currency_rate_cmd);
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
        let open_friend_cmd = OpenFriendCurrencyCmd {
            friend_name: format!("node{}", 1 - j),
            currency_name: "FST".to_owned(),
        };
        let config_cmd = ConfigCmd::OpenFriendCurrency(open_friend_cmd);
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

/// Close requests and disable friends
fn add_remove_currency(stctrl_setup: &StCtrlSetup) {
    // Set rate (To add a currency)
    // ----------------------------
    for j in 0..1 {
        let set_friend_currency_rate_cmd = SetFriendCurrencyRateCmd {
            friend_name: format!("node{}", 1 - j),
            currency_name: "FST2".to_owned(),
            mul: 0,
            add: 1,
        };
        let config_cmd = ConfigCmd::SetFriendCurrencyRate(set_friend_currency_rate_cmd);
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

    thread::sleep(time::Duration::from_millis(100));

    // Remove the currency we have just added
    // --------------------------------------
    for j in 0..1 {
        let remove_friend_currency_cmd = RemoveFriendCurrencyCmd {
            friend_name: format!("node{}", 1 - j),
            currency_name: "FST2".to_owned(),
        };
        let config_cmd = ConfigCmd::RemoveFriendCurrency(remove_friend_currency_cmd);
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
}

/// Set max_debt for node1
fn set_max_debt(stctrl_setup: &StCtrlSetup) {
    // node0 sets remote max debt for node1:
    let set_friend_currency_max_debt_cmd = SetFriendCurrencyMaxDebtCmd {
        friend_name: "node1".to_owned(),
        currency_name: "FST".to_owned(),
        max_debt: 200,
    };
    let config_cmd = ConfigCmd::SetFriendCurrencyMaxDebt(set_friend_currency_max_debt_cmd);
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
}

/// Create and cancel an invoice, just to make sure the API is operational.
/// Node0: create an invoice
/// Node0: cancel invoice
fn create_cancel_invoice(stctrl_setup: &StCtrlSetup) {
    // Node0: generate an invoice:
    // ---------------------------
    let create_invoice_cmd = CreateInvoiceCmd {
        currency_name: "FST".to_owned(),
        amount: 50,
        invoice_path: stctrl_setup
            .temp_dir_path
            .join("node0")
            .join("temp_invoice.invoice"),
    };
    let seller_cmd = SellerCmd::CreateInvoice(create_invoice_cmd);
    let subcommand = StCtrlSubcommand::Seller(seller_cmd);

    let st_ctrl_cmd = StCtrlCmd {
        idfile: stctrl_setup.temp_dir_path.join("app0").join("app0.ident"),
        node_ticket: stctrl_setup
            .temp_dir_path
            .join("node0")
            .join("node0.ticket"),
        subcommand,
    };
    stctrl(st_ctrl_cmd.clone(), &mut Vec::new()).unwrap();

    // Node0: cancel the invoice:
    // ---------------------------
    let cancel_invoice_cmd = CancelInvoiceCmd {
        invoice_path: stctrl_setup
            .temp_dir_path
            .join("node0")
            .join("temp_invoice.invoice"),
    };
    let seller_cmd = SellerCmd::CancelInvoice(cancel_invoice_cmd);
    let subcommand = StCtrlSubcommand::Seller(seller_cmd);

    let st_ctrl_cmd = StCtrlCmd {
        idfile: stctrl_setup.temp_dir_path.join("app0").join("app0.ident"),
        node_ticket: stctrl_setup
            .temp_dir_path
            .join("node0")
            .join("node0.ticket"),
        subcommand,
    };
    stctrl(st_ctrl_cmd.clone(), &mut Vec::new()).unwrap();

    // Cancel invoice should remove the invoice file:
    assert!(!stctrl_setup
        .temp_dir_path
        .join("node0")
        .join("temp_invoice.invoice")
        .exists());
}

/// Node0: generate an invoice
/// Node1: pay the invoice
/// Node0: Commit invoice
/// Node1: Wait for receipt
fn pay_invoice(stctrl_setup: &StCtrlSetup) {
    // Node0: generate an invoice:
    // ---------------------------
    let create_invoice_cmd = CreateInvoiceCmd {
        currency_name: "FST".to_owned(),
        amount: 50,
        invoice_path: stctrl_setup
            .temp_dir_path
            .join("node0")
            .join("test1.invoice"),
    };
    let seller_cmd = SellerCmd::CreateInvoice(create_invoice_cmd);
    let subcommand = StCtrlSubcommand::Seller(seller_cmd);

    let st_ctrl_cmd = StCtrlCmd {
        idfile: stctrl_setup.temp_dir_path.join("app0").join("app0.ident"),
        node_ticket: stctrl_setup
            .temp_dir_path
            .join("node0")
            .join("node0.ticket"),
        subcommand,
    };
    stctrl(st_ctrl_cmd.clone(), &mut Vec::new()).unwrap();

    // Node1: pay the invoice:
    // -----------------------
    loop {
        let pay_invoice_cmd = PayInvoiceCmd {
            invoice_path: stctrl_setup
                .temp_dir_path
                .join("node0")
                .join("test1.invoice"),
            payment_path: stctrl_setup
                .temp_dir_path
                .join("node1")
                .join("test1.payment"),
            commit_path: stctrl_setup
                .temp_dir_path
                .join("node1")
                .join("test1.commit"),
        };
        let buyer_cmd = BuyerCmd::PayInvoice(pay_invoice_cmd);
        let subcommand = StCtrlSubcommand::Buyer(buyer_cmd);

        let st_ctrl_cmd = StCtrlCmd {
            idfile: stctrl_setup.temp_dir_path.join("app1").join("app1.ident"),
            node_ticket: stctrl_setup
                .temp_dir_path
                .join("node1")
                .join("node1.ticket"),
            subcommand,
        };

        // We should try again if no suitable route was found:
        match stctrl(st_ctrl_cmd.clone(), &mut Vec::new()) {
            Ok(_) => break,
            Err(StCtrlError::BuyerError(BuyerError::NoSuitableRoute))
            | Err(StCtrlError::BuyerError(BuyerError::AppRoutesError)) => {}
            Err(_) => {
                unreachable!();
            }
        }
        thread::sleep(time::Duration::from_millis(100));
    }

    // Node0: Commit the invoice:
    let commit_invoice_cmd = CommitInvoiceCmd {
        invoice_path: stctrl_setup
            .temp_dir_path
            .join("node0")
            .join("test1.invoice"),
        commit_path: stctrl_setup
            .temp_dir_path
            .join("node1")
            .join("test1.commit"),
    };

    let seller_cmd = SellerCmd::CommitInvoice(commit_invoice_cmd);
    let subcommand = StCtrlSubcommand::Seller(seller_cmd);

    let st_ctrl_cmd = StCtrlCmd {
        idfile: stctrl_setup.temp_dir_path.join("app0").join("app0.ident"),
        node_ticket: stctrl_setup
            .temp_dir_path
            .join("node0")
            .join("node0.ticket"),
        subcommand,
    };
    stctrl(st_ctrl_cmd.clone(), &mut Vec::new()).unwrap();

    // Node1: Wait for a receipt:
    // -----------------------
    let payment_status_cmd = PaymentStatusCmd {
        payment_path: stctrl_setup
            .temp_dir_path
            .join("node1")
            .join("test1.payment"),
        receipt_path: stctrl_setup
            .temp_dir_path
            .join("node1")
            .join("test1.receipt"),
    };
    let buyer_cmd = BuyerCmd::PaymentStatus(payment_status_cmd);
    let subcommand = StCtrlSubcommand::Buyer(buyer_cmd);

    let st_ctrl_cmd = StCtrlCmd {
        idfile: stctrl_setup.temp_dir_path.join("app1").join("app1.ident"),
        node_ticket: stctrl_setup
            .temp_dir_path
            .join("node1")
            .join("node1.ticket"),
        subcommand,
    };

    // Node1: Keep asking, until we get a receipt:
    loop {
        let mut output = Vec::new();
        stctrl(st_ctrl_cmd.clone(), &mut output).unwrap();
        let output_string = str::from_utf8(&output).unwrap();
        if output_string.contains("Saving receipt to file.") {
            break;
        }
        thread::sleep(time::Duration::from_millis(100));
    }

    // Make sure that the payment file was removed:
    assert!(!stctrl_setup
        .temp_dir_path
        .join("node1")
        .join("test1.payment")
        .exists());

    // Verify the receipt:
    // ------------------
    let verify_receipt_cmd = VerifyReceiptCmd {
        invoice_path: stctrl_setup
            .temp_dir_path
            .join("node0")
            .join("test1.invoice"),
        receipt_path: stctrl_setup
            .temp_dir_path
            .join("node1")
            .join("test1.receipt"),
    };

    let stverify_cmd = StVerifyCmd::VerifyReceipt(verify_receipt_cmd);
    let mut output = Vec::new();
    stverify(stverify_cmd, &mut output).unwrap();
    assert!(str::from_utf8(&output).unwrap().contains("is valid!"));
}

/*
/// View balance of node1
fn check_balance(stctrl_setup: &StCtrlSetup) {
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
    stctrl(st_ctrl_cmd, &mut output).unwrap();
    assert!(str::from_utf8(&output).unwrap().contains("-70"));
}
*/

/// Export a friend's last token and then verify it
fn export_token(stctrl_setup: &StCtrlSetup) {
    // node1: Get node0's last token:
    let friend_last_token_cmd = FriendLastTokenCmd {
        friend_name: "node0".to_owned(),
        token_path: stctrl_setup.temp_dir_path.join("node1").join("node0.token"),
    };
    let info_cmd = InfoCmd::FriendLastToken(friend_last_token_cmd);
    let subcommand = StCtrlSubcommand::Info(info_cmd);

    let st_ctrl_cmd = StCtrlCmd {
        idfile: stctrl_setup.temp_dir_path.join("app1").join("app1.ident"),
        node_ticket: stctrl_setup
            .temp_dir_path
            .join("node1")
            .join("node1.ticket"),
        subcommand,
    };
    stctrl(st_ctrl_cmd, &mut Vec::new()).unwrap();

    // Verify the token:
    // ------------------
    let verify_token_cmd = VerifyTokenCmd {
        token_path: stctrl_setup.temp_dir_path.join("node1").join("node0.token"),
    };

    let stverify_cmd = StVerifyCmd::VerifyToken(verify_token_cmd);
    let mut output = Vec::new();
    stverify(stverify_cmd, &mut output).unwrap();
    let output_str = str::from_utf8(&output).unwrap();
    assert!(output_str.contains("is valid!"));
    assert!(output_str.contains("balance=50"));
}

/// Close requests and disable friends
fn close_disable(stctrl_setup: &StCtrlSetup) {
    // Close friends:
    // ---------------
    for j in 0..2 {
        let close_friend_currency_cmd = CloseFriendCurrencyCmd {
            friend_name: format!("node{}", 1 - j),
            currency_name: "FST".to_owned(),
        };
        let config_cmd = ConfigCmd::CloseFriendCurrency(close_friend_currency_cmd);
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

    // Wait until requests are seen closed:
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
            if output_string.contains("LR=-") && output_string.contains("RR=-") {
                break;
            }
            thread::sleep(time::Duration::from_millis(100));
        }
    }

    // Remove currencies:
    // This should not work, because the currency is already in use.
    // ------------------
    for j in 0..2 {
        let remove_friend_currency_cmd = RemoveFriendCurrencyCmd {
            friend_name: format!("node{}", 1 - j),
            currency_name: "FST".to_owned(),
        };
        let config_cmd = ConfigCmd::RemoveFriendCurrency(remove_friend_currency_cmd);
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

    // Nodes disable each other (as friends):
    for j in 0..2 {
        let disable_friend_cmd = DisableFriendCmd {
            friend_name: format!("node{}", 1 - j),
        };
        let config_cmd = ConfigCmd::DisableFriend(disable_friend_cmd);
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

    // Wait until friends are seen disabled and offline:
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

        // Wait until the remote friend seems to be disabled and offline:
        loop {
            let mut output = Vec::new();
            stctrl(st_ctrl_cmd.clone(), &mut output).unwrap();
            let output_string = str::from_utf8(&output).unwrap();
            if output_string.contains("D-") {
                break;
            }
            thread::sleep(time::Duration::from_millis(100));
        }
    }
}

#[test]
fn basic_cli() {
    let _ = env_logger::init();

    // Create a temporary directory.
    // Should be deleted when gets out of scope:
    let temp_dir = tempdir().unwrap();
    let temp_dir_path = temp_dir.path().to_path_buf();
    let stctrl_setup = create_stctrl_setup(&temp_dir_path);

    spawn_entities(&stctrl_setup);
    // Wait some time, letting the index servers exchange time hashes:
    thread::sleep(time::Duration::from_millis(500));

    configure_mutual_credit(&stctrl_setup);
    add_remove_currency(&stctrl_setup);
    set_max_debt(&stctrl_setup);
    create_cancel_invoice(&stctrl_setup);
    pay_invoice(&stctrl_setup);
    // check_balance(&stctrl_setup);
    export_token(&stctrl_setup);
    close_disable(&stctrl_setup);
}
