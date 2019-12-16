use std::cmp::Ordering;
use std::convert::TryFrom;

use super::utils::apply_funder_incoming;

use futures::executor::{LocalPool, ThreadPool};
use futures::task::SpawnExt;
use futures::{future, FutureExt};

use identity::{create_identity, IdentityClient};

use crypto::identity::{compare_public_key, SoftwareEd25519Identity};
use crypto::rand::{RngContainer, RandGen};
use crypto::test_utils::DummyRandom;

use proto::crypto::{InvoiceId, PaymentId, Uid, PrivateKey};

use proto::funder::messages::{
    AckClosePayment, AddFriend, AddInvoice, CreatePayment, CreateTransaction, Currency,
    FriendMessage, FriendStatus, FriendsRoute, FunderControl, FunderIncomingControl,
    FunderOutgoingControl, PaymentStatus, Rate, RequestResult, RequestsStatus,
    SetFriendCurrencyMaxDebt, SetFriendCurrencyRate, SetFriendCurrencyRequestsStatus,
    SetFriendStatus,
};

use crate::ephemeral::Ephemeral;
use crate::friend::ChannelStatus;
use crate::state::FunderState;
use crate::types::{
    ChannelerConfig, FunderIncoming, FunderIncomingComm, FunderOutgoingComm,
    IncomingLivenessMessage,
};

use crate::tests::utils::{dummy_named_relay_address, dummy_relay_address};

async fn task_handler_pair_basic<'a>(
    identity_client1: &'a mut IdentityClient,
    identity_client2: &'a mut IdentityClient,
) {
    // NOTE: We use Box::pin() in order to make sure we don't get a too large Future which will
    // cause a stack overflow.
    // See:  https://github.com/rust-lang-nursery/futures-rs/issues/1330

    let currency = Currency::try_from("FST".to_owned()).unwrap();

    // Sort the identities. identity_client1 will be the first sender:
    let pk1 = identity_client1.request_public_key().await.unwrap();
    let pk2 = identity_client2.request_public_key().await.unwrap();
    let (identity_client1, pk1, identity_client2, pk2) =
        if compare_public_key(&pk1, &pk2) == Ordering::Less {
            (identity_client1, pk1, identity_client2, pk2)
        } else {
            (identity_client2, pk2, identity_client1, pk1)
        };

    let relays1 = vec![dummy_named_relay_address(1)];
    let mut state1 = FunderState::<u32>::new(pk1.clone(), relays1);
    let mut ephemeral1 = Ephemeral::new();
    let relays2 = vec![dummy_named_relay_address(2)];
    let mut state2 = FunderState::<u32>::new(pk2.clone(), relays2);
    let mut ephemeral2 = Ephemeral::new();

    let mut rng = RngContainer::new(DummyRandom::new(&[3u8]));

    // Initialize 1:
    let funder_incoming = FunderIncoming::Init;
    Box::pin(apply_funder_incoming(
        funder_incoming,
        &mut state1,
        &mut ephemeral1,
        &mut rng,
        identity_client1,
    ))
    .await
    .unwrap();

    // Initialize 2:
    let funder_incoming = FunderIncoming::Init;
    Box::pin(apply_funder_incoming(
        funder_incoming,
        &mut state2,
        &mut ephemeral2,
        &mut rng,
        identity_client2,
    ))
    .await
    .unwrap();

    // Node1: Add friend 2:
    let add_friend = AddFriend {
        friend_public_key: pk2.clone(),
        relays: vec![dummy_relay_address(2)],
        name: String::from("pk2"),
    };
    let incoming_control_message = FunderIncomingControl::new(
        Uid::from(&[11; Uid::len()]),
        FunderControl::AddFriend(add_friend),
    );
    let funder_incoming = FunderIncoming::Control(incoming_control_message);
    Box::pin(apply_funder_incoming(
        funder_incoming,
        &mut state1,
        &mut ephemeral1,
        &mut rng,
        identity_client1,
    ))
    .await
    .unwrap();

    // Node1: Enable friend 2:
    let set_friend_status = SetFriendStatus {
        friend_public_key: pk2.clone(),
        status: FriendStatus::Enabled,
    };
    let incoming_control_message = FunderIncomingControl::new(
        Uid::from(&[12; Uid::len()]),
        FunderControl::SetFriendStatus(set_friend_status),
    );
    let funder_incoming = FunderIncoming::Control(incoming_control_message);
    Box::pin(apply_funder_incoming(
        funder_incoming,
        &mut state1,
        &mut ephemeral1,
        &mut rng,
        identity_client1,
    ))
    .await
    .unwrap();

    // Node2: Add friend 1:
    let add_friend = AddFriend {
        friend_public_key: pk1.clone(),
        relays: vec![dummy_relay_address(1)],
        name: String::from("pk1"),
    };
    let incoming_control_message = FunderIncomingControl::new(
        Uid::from(&[13; Uid::len()]),
        FunderControl::AddFriend(add_friend),
    );
    let funder_incoming = FunderIncoming::Control(incoming_control_message);
    Box::pin(apply_funder_incoming(
        funder_incoming,
        &mut state2,
        &mut ephemeral2,
        &mut rng,
        identity_client2,
    ))
    .await
    .unwrap();

    // Node2: enable friend 1:
    let set_friend_status = SetFriendStatus {
        friend_public_key: pk1.clone(),
        status: FriendStatus::Enabled,
    };
    let incoming_control_message = FunderIncomingControl::new(
        Uid::from(&[14; Uid::len()]),
        FunderControl::SetFriendStatus(set_friend_status),
    );
    let funder_incoming = FunderIncoming::Control(incoming_control_message);
    Box::pin(apply_funder_incoming(
        funder_incoming,
        &mut state2,
        &mut ephemeral2,
        &mut rng,
        identity_client2,
    ))
    .await
    .unwrap();

    // Node1: Notify that Node2 is alive
    // We expect that Node1 will resend his outgoing message when he is notified that Node1 is online.
    let incoming_liveness_message = IncomingLivenessMessage::Online(pk2.clone());
    let funder_incoming =
        FunderIncoming::Comm(FunderIncomingComm::Liveness(incoming_liveness_message));
    let (outgoing_comms, _outgoing_control) = Box::pin(apply_funder_incoming(
        funder_incoming,
        &mut state1,
        &mut ephemeral1,
        &mut rng,
        identity_client1,
    ))
    .await
    .unwrap();

    assert_eq!(outgoing_comms.len(), 1);
    let friend_message = match &outgoing_comms[0] {
        FunderOutgoingComm::FriendMessage((pk, friend_message)) => {
            if let FriendMessage::MoveTokenRequest(move_token_request) = friend_message {
                assert_eq!(pk, &pk2);
                // Token is wanted because Node1 wants to send his configured address later.
                assert_eq!(move_token_request.token_wanted, true);

                let friend_move_token = &move_token_request.move_token;
                // assert_eq!(friend_move_token.move_token_counter, 0);
                // assert_eq!(friend_move_token.inconsistency_counter, 0);
                // assert_eq!(friend_move_token.balance, 0);
                assert!(friend_move_token.opt_local_relays.is_none());
            } else {
                unreachable!();
            }
            friend_message.clone()
        }
        _ => unreachable!(),
    };

    // Node2: Notify that Node1 is alive
    let incoming_liveness_message = IncomingLivenessMessage::Online(pk1.clone());
    let funder_incoming =
        FunderIncoming::Comm(FunderIncomingComm::Liveness(incoming_liveness_message));
    let (outgoing_comms, _outgoing_control) = Box::pin(apply_funder_incoming(
        funder_incoming,
        &mut state2,
        &mut ephemeral2,
        &mut rng,
        identity_client2,
    ))
    .await
    .unwrap();

    // Node2 sends information about his address to Node1, and updates channeler
    assert_eq!(outgoing_comms.len(), 2);

    match &outgoing_comms[0] {
        FunderOutgoingComm::ChannelerConfig(ChannelerConfig::UpdateFriend(update_friend)) => {
            assert_eq!(update_friend.friend_public_key, pk1);
            assert_eq!(update_friend.friend_relays, vec![dummy_relay_address(1)]);
            assert_eq!(update_friend.local_relays, vec![dummy_relay_address(2)]);
        }
        _ => unreachable!(),
    };

    // Node2: Receive MoveToken from Node1:
    // (Node2 should be able to discard this duplicate message)
    let funder_incoming =
        FunderIncoming::Comm(FunderIncomingComm::Friend((pk1.clone(), friend_message)));
    let (outgoing_comms, _outgoing_control) = Box::pin(apply_funder_incoming(
        funder_incoming,
        &mut state2,
        &mut ephemeral2,
        &mut rng,
        identity_client2,
    ))
    .await
    .unwrap();

    // The same message should be again sent by Node2:
    assert_eq!(outgoing_comms.len(), 1);

    let friend_message = match &outgoing_comms[0] {
        FunderOutgoingComm::FriendMessage((pk, friend_message)) => {
            if let FriendMessage::MoveTokenRequest(move_token_request) = friend_message {
                assert_eq!(pk, &pk1);
                assert_eq!(move_token_request.token_wanted, true);

                let friend_move_token = &move_token_request.move_token;
                // assert_eq!(friend_move_token.move_token_counter, 1);
                // assert_eq!(friend_move_token.inconsistency_counter, 0);
                // assert_eq!(friend_move_token.balance, 0);
                assert_eq!(
                    friend_move_token.opt_local_relays,
                    Some(vec![dummy_relay_address(2)])
                );
            } else {
                unreachable!();
            }
            friend_message.clone()
        }
        _ => unreachable!(),
    };

    // Node1: Receive the message from Node2 (Setting address):
    let funder_incoming =
        FunderIncoming::Comm(FunderIncomingComm::Friend((pk2.clone(), friend_message)));
    let (outgoing_comms, _outgoing_control) = Box::pin(apply_funder_incoming(
        funder_incoming,
        &mut state1,
        &mut ephemeral1,
        &mut rng,
        identity_client1,
    ))
    .await
    .unwrap();

    assert_eq!(outgoing_comms.len(), 2);
    // Node1 should send a message containing opt_local_relays to Node2:
    let friend_message = match &outgoing_comms[1] {
        FunderOutgoingComm::FriendMessage((pk, friend_message)) => {
            if let FriendMessage::MoveTokenRequest(move_token_request) = friend_message {
                assert_eq!(pk, &pk2);
                assert_eq!(move_token_request.token_wanted, true);

                let friend_move_token = &move_token_request.move_token;
                // assert_eq!(friend_move_token.move_token_counter, 2);
                // assert_eq!(friend_move_token.inconsistency_counter, 0);
                // assert_eq!(friend_move_token.balance, 0);
                assert_eq!(
                    friend_move_token.opt_local_relays,
                    Some(vec![dummy_relay_address(1)])
                );
            } else {
                unreachable!();
            }
            friend_message.clone()
        }
        _ => unreachable!(),
    };

    // Node2: Receive the message from Node1 (Setting address):
    let funder_incoming =
        FunderIncoming::Comm(FunderIncomingComm::Friend((pk1.clone(), friend_message)));
    let (outgoing_comms, _outgoing_control) = Box::pin(apply_funder_incoming(
        funder_incoming,
        &mut state2,
        &mut ephemeral2,
        &mut rng,
        identity_client2,
    ))
    .await
    .unwrap();

    assert_eq!(outgoing_comms.len(), 1);

    // Node2 sends an empty move token to node1:
    let friend_message = match &outgoing_comms[0] {
        FunderOutgoingComm::FriendMessage((pk, friend_message)) => {
            if let FriendMessage::MoveTokenRequest(move_token_request) = friend_message {
                assert_eq!(pk, &pk1);
                assert_eq!(move_token_request.token_wanted, false);

                let friend_move_token = &move_token_request.move_token;
                // assert_eq!(friend_move_token.move_token_counter, 3);
                // assert_eq!(friend_move_token.inconsistency_counter, 0);
                // assert_eq!(friend_move_token.balance, 0);
                assert_eq!(friend_move_token.opt_local_relays, None);
            } else {
                unreachable!();
            }
            friend_message.clone()
        }
        _ => unreachable!(),
    };

    // Node1: Receives the empty move token message from Node2:
    let funder_incoming =
        FunderIncoming::Comm(FunderIncomingComm::Friend((pk2.clone(), friend_message)));
    let (outgoing_comms, _outgoing_control) = Box::pin(apply_funder_incoming(
        funder_incoming,
        &mut state1,
        &mut ephemeral1,
        &mut rng,
        identity_client1,
    ))
    .await
    .unwrap();

    assert!(outgoing_comms.is_empty());

    // Node1 receives control message to add a currency:
    let set_friend_currency_rate = SetFriendCurrencyRate {
        friend_public_key: pk2.clone(),
        currency: currency.clone(),
        rate: Rate::new(),
    };
    let incoming_control_message = FunderIncomingControl::new(
        Uid::from(&[16; Uid::len()]),
        FunderControl::SetFriendCurrencyRate(set_friend_currency_rate),
    );
    let funder_incoming = FunderIncoming::Control(incoming_control_message);
    let (outgoing_comms, _outgoing_control) = Box::pin(apply_funder_incoming(
        funder_incoming,
        &mut state1,
        &mut ephemeral1,
        &mut rng,
        identity_client1,
    ))
    .await
    .unwrap();

    // Node1 produces outgoing communication (Adding an active currency):
    assert_eq!(outgoing_comms.len(), 1);
    let friend_message = match &outgoing_comms[0] {
        FunderOutgoingComm::FriendMessage((pk, friend_message)) => {
            if let FriendMessage::MoveTokenRequest(move_token_request) = friend_message {
                assert_eq!(pk, &pk2);
                assert_eq!(move_token_request.token_wanted, false);
            } else {
                unreachable!();
            }
            friend_message.clone()
        }
        _ => unreachable!(),
    };

    // Node2 gets a MoveToken message from Node1, asking to add an active currency:
    let funder_incoming =
        FunderIncoming::Comm(FunderIncomingComm::Friend((pk1.clone(), friend_message)));
    let (_outgoing_comms, _outgoing_control) = Box::pin(apply_funder_incoming(
        funder_incoming,
        &mut state2,
        &mut ephemeral2,
        &mut rng,
        identity_client2,
    ))
    .await
    .unwrap();

    // Node2 receives control message to add a currency:
    let set_friend_currency_rate = SetFriendCurrencyRate {
        friend_public_key: pk1.clone(),
        currency: currency.clone(),
        rate: Rate::new(),
    };
    let incoming_control_message = FunderIncomingControl::new(
        Uid::from(&[17; Uid::len()]),
        FunderControl::SetFriendCurrencyRate(set_friend_currency_rate),
    );
    let funder_incoming = FunderIncoming::Control(incoming_control_message);
    let (outgoing_comms, _outgoing_control) = Box::pin(apply_funder_incoming(
        funder_incoming,
        &mut state2,
        &mut ephemeral2,
        &mut rng,
        identity_client2,
    ))
    .await
    .unwrap();

    // Node2 produces outgoing communication (Adding an active currency):
    assert_eq!(outgoing_comms.len(), 1);
    let friend_message = match &outgoing_comms[0] {
        FunderOutgoingComm::FriendMessage((pk, friend_message)) => {
            if let FriendMessage::MoveTokenRequest(move_token_request) = friend_message {
                assert_eq!(pk, &pk1);
                assert_eq!(move_token_request.token_wanted, false);
            } else {
                unreachable!();
            }
            friend_message.clone()
        }
        _ => unreachable!(),
    };

    // Node1 gets a MoveToken message from Node2, asking to add an active currency:
    let funder_incoming =
        FunderIncoming::Comm(FunderIncomingComm::Friend((pk2.clone(), friend_message)));
    let (_outgoing_comms, _outgoing_control) = Box::pin(apply_funder_incoming(
        funder_incoming,
        &mut state1,
        &mut ephemeral1,
        &mut rng,
        identity_client1,
    ))
    .await
    .unwrap();

    // Node1 receives control message to set remote max debt.
    let set_friend_currency_max_debt = SetFriendCurrencyMaxDebt {
        friend_public_key: pk2.clone(),
        currency: currency.clone(),
        remote_max_debt: 100,
    };
    let incoming_control_message = FunderIncomingControl::new(
        Uid::from(&[15; Uid::len()]),
        FunderControl::SetFriendCurrencyMaxDebt(set_friend_currency_max_debt),
    );
    let funder_incoming = FunderIncoming::Control(incoming_control_message);
    let (outgoing_comms, _outgoing_control) = Box::pin(apply_funder_incoming(
        funder_incoming,
        &mut state1,
        &mut ephemeral1,
        &mut rng,
        identity_client1,
    ))
    .await
    .unwrap();

    // Node1 will send the SetRemoteMaxDebt message to Node2:
    assert_eq!(outgoing_comms.len(), 1);
    let friend_message = match &outgoing_comms[0] {
        FunderOutgoingComm::FriendMessage((pk, friend_message)) => {
            if let FriendMessage::MoveTokenRequest(move_token_request) = friend_message {
                assert_eq!(pk, &pk2);
                assert_eq!(move_token_request.token_wanted, false);
            } else {
                unreachable!();
            }
            friend_message.clone()
        }
        _ => unreachable!(),
    };

    // Node2: Receive friend_message (With SetRemoteMaxDebt) from Node1:
    let funder_incoming =
        FunderIncoming::Comm(FunderIncomingComm::Friend((pk1.clone(), friend_message)));
    let (_outgoing_comms, _outgoing_control) = Box::pin(apply_funder_incoming(
        funder_incoming,
        &mut state2,
        &mut ephemeral2,
        &mut rng,
        identity_client2,
    ))
    .await
    .unwrap();

    let friend2 = state1.friends.get(&pk2).unwrap();
    let remote_max_debt = match &friend2.channel_status {
        ChannelStatus::Consistent(channel_consistent) => {
            channel_consistent
                .token_channel
                .get_mutual_credits()
                .get(&currency)
                .unwrap()
                .state()
                .balance
                .remote_max_debt
        }
        _ => unreachable!(),
    };
    assert_eq!(remote_max_debt, 100);

    let friend1 = state2.friends.get(&pk1).unwrap();
    let local_max_debt = match &friend1.channel_status {
        ChannelStatus::Consistent(channel_consistent) => {
            channel_consistent
                .token_channel
                .get_mutual_credits()
                .get(&currency)
                .unwrap()
                .state()
                .balance
                .local_max_debt
        }
        _ => unreachable!(),
    };
    assert_eq!(local_max_debt, 100);

    // Node1 opens an invoice (To get payment from Node2):
    let add_invoice = AddInvoice {
        invoice_id: InvoiceId::from(&[1u8; InvoiceId::len()]),
        currency: currency.clone(),
        total_dest_payment: 16,
    };

    let incoming_control_message = FunderIncomingControl::new(
        Uid::from(&[16; Uid::len()]),
        FunderControl::AddInvoice(add_invoice),
    );
    let funder_incoming = FunderIncoming::Control(incoming_control_message);
    let (outgoing_comms, outgoing_control) = Box::pin(apply_funder_incoming(
        funder_incoming,
        &mut state1,
        &mut ephemeral1,
        &mut rng,
        identity_client1,
    ))
    .await
    .unwrap();

    assert_eq!(outgoing_comms.len(), 0);
    // SetNumOpenInvoices(1):
    assert_eq!(outgoing_control.len(), 1);

    // Node2 opens payment (to Node1) according to the invoice from Node1:
    let create_payment = CreatePayment {
        payment_id: PaymentId::from(&[3u8; PaymentId::len()]),
        invoice_id: InvoiceId::from(&[1u8; InvoiceId::len()]),
        currency: currency.clone(),
        total_dest_payment: 16,
        dest_public_key: pk1.clone(),
    };

    let incoming_control_message = FunderIncomingControl::new(
        Uid::from(&[17; Uid::len()]),
        FunderControl::CreatePayment(create_payment),
    );
    let funder_incoming = FunderIncoming::Control(incoming_control_message);
    let (outgoing_comms, outgoing_control) = Box::pin(apply_funder_incoming(
        funder_incoming,
        &mut state2,
        &mut ephemeral2,
        &mut rng,
        identity_client2,
    ))
    .await
    .unwrap();

    assert_eq!(outgoing_comms.len(), 0);
    // SetNumPayments(1):
    assert_eq!(outgoing_control.len(), 1);

    // Node2 creates a transaction to send funds to Node1:
    let create_transaction = CreateTransaction {
        payment_id: PaymentId::from(&[3u8; PaymentId::len()]),
        request_id: Uid::from(&[0; Uid::len()]),
        route: FriendsRoute {
            public_keys: vec![pk2.clone(), pk1.clone()],
        },
        dest_payment: 16,
        fees: 4,
    };

    let incoming_control_message = FunderIncomingControl::new(
        Uid::from(&[18; Uid::len()]),
        FunderControl::CreateTransaction(create_transaction),
    );
    let funder_incoming = FunderIncoming::Control(incoming_control_message);
    let (outgoing_comms, outgoing_control) = Box::pin(apply_funder_incoming(
        funder_incoming,
        &mut state2,
        &mut ephemeral2,
        &mut rng,
        identity_client2,
    ))
    .await
    .unwrap();

    assert_eq!(outgoing_comms.len(), 0);
    // SetNumPayments(1):
    assert_eq!(outgoing_control.len(), 2);
    let outgoing = &outgoing_control[1];
    let transaction_result = match outgoing {
        FunderOutgoingControl::TransactionResult(transaction_result) => transaction_result,
        _ => unreachable!(),
    };

    // We expect failure, because remote side is not ready:
    assert_eq!(transaction_result.request_id, Uid::from(&[0; Uid::len()]));
    match transaction_result.result {
        RequestResult::Failure => {}
        _ => unreachable!(),
    };

    // Checking the current requests status on the mutual credit:
    let friend2 = state1.friends.get(&pk2).unwrap();
    let mutual_credit_state = match &friend2.channel_status {
        ChannelStatus::Consistent(channel_consistent) => channel_consistent
            .token_channel
            .get_mutual_credits()
            .get(&currency)
            .unwrap()
            .state(),
        _ => unreachable!(),
    };
    assert_eq!(
        mutual_credit_state.requests_status.local,
        RequestsStatus::Closed
    );
    assert_eq!(
        mutual_credit_state.requests_status.remote,
        RequestsStatus::Closed
    );

    // Node1 gets a control message to declare his requests are open,
    // However, Node1 doesn't have the token at this moment.
    let set_requests_status = SetFriendCurrencyRequestsStatus {
        friend_public_key: pk2.clone(),
        currency: currency.clone(),
        status: RequestsStatus::Open,
    };
    let incoming_control_message = FunderIncomingControl::new(
        Uid::from(&[19; Uid::len()]),
        FunderControl::SetFriendCurrencyRequestsStatus(set_requests_status),
    );
    let funder_incoming = FunderIncoming::Control(incoming_control_message);
    let (outgoing_comms, _outgoing_control) = Box::pin(apply_funder_incoming(
        funder_incoming,
        &mut state1,
        &mut ephemeral1,
        &mut rng,
        identity_client1,
    ))
    .await
    .unwrap();

    // Node1 will request the token:
    assert_eq!(outgoing_comms.len(), 1);
    let friend_message =
        if let FunderOutgoingComm::FriendMessage((_pk, friend_message)) = &outgoing_comms[0] {
            friend_message.clone()
        } else {
            unreachable!();
        };

    // Node2 receives the request_token message:
    let funder_incoming =
        FunderIncoming::Comm(FunderIncomingComm::Friend((pk1.clone(), friend_message)));
    let (outgoing_comms, _outgoing_control) = Box::pin(apply_funder_incoming(
        funder_incoming,
        &mut state2,
        &mut ephemeral2,
        &mut rng,
        identity_client2,
    ))
    .await
    .unwrap();

    assert_eq!(outgoing_comms.len(), 1);
    let friend_message =
        if let FunderOutgoingComm::FriendMessage((_pk, friend_message)) = &outgoing_comms[0] {
            friend_message.clone()
        } else {
            unreachable!();
        };

    // Node1 receives the token from Node2:
    let funder_incoming =
        FunderIncoming::Comm(FunderIncomingComm::Friend((pk2.clone(), friend_message)));
    let (outgoing_comms, _outgoing_control) = Box::pin(apply_funder_incoming(
        funder_incoming,
        &mut state1,
        &mut ephemeral1,
        &mut rng,
        identity_client1,
    ))
    .await
    .unwrap();

    // Node1 declares that his requests are open:
    let friend_message =
        if let FunderOutgoingComm::FriendMessage((_pk, friend_message)) = &outgoing_comms[0] {
            friend_message.clone()
        } else {
            unreachable!();
        };

    // Node2 receives the set requests open message:
    let funder_incoming =
        FunderIncoming::Comm(FunderIncomingComm::Friend((pk1.clone(), friend_message)));
    let (_outgoing_comms, _outgoing_control) = Box::pin(apply_funder_incoming(
        funder_incoming,
        &mut state2,
        &mut ephemeral2,
        &mut rng,
        identity_client2,
    ))
    .await
    .unwrap();

    // Checking the current requests status on the mutual credit for Node1:
    let friend2 = state1.friends.get(&pk2).unwrap();
    let mutual_credit_state = match &friend2.channel_status {
        ChannelStatus::Consistent(channel_consistent) => channel_consistent
            .token_channel
            .get_mutual_credits()
            .get(&currency)
            .unwrap()
            .state(),
        _ => unreachable!(),
    };
    assert!(mutual_credit_state.requests_status.local.is_open());
    assert!(!mutual_credit_state.requests_status.remote.is_open());

    // Checking the current requests status on the mutual credit for Node2:
    let friend1 = state2.friends.get(&pk1).unwrap();
    let mutual_credit_state = match &friend1.channel_status {
        ChannelStatus::Consistent(channel_consistent) => channel_consistent
            .token_channel
            .get_mutual_credits()
            .get(&currency)
            .unwrap()
            .state(),
        _ => unreachable!(),
    };
    assert!(!mutual_credit_state.requests_status.local.is_open());
    assert!(mutual_credit_state.requests_status.remote.is_open());

    // Node2 again creates a transaction to send funds to Node1:
    let create_transaction = CreateTransaction {
        payment_id: PaymentId::from(&[3u8; PaymentId::len()]),
        request_id: Uid::from(&[1; Uid::len()]),
        route: FriendsRoute {
            public_keys: vec![pk2.clone(), pk1.clone()],
        },
        dest_payment: 16,
        fees: 4,
    };

    let incoming_control_message = FunderIncomingControl::new(
        Uid::from(&[20; Uid::len()]),
        FunderControl::CreateTransaction(create_transaction),
    );
    let funder_incoming = FunderIncoming::Control(incoming_control_message);
    let (outgoing_comms, outgoing_control) = Box::pin(apply_funder_incoming(
        funder_incoming,
        &mut state2,
        &mut ephemeral2,
        &mut rng,
        identity_client2,
    ))
    .await
    .unwrap();

    assert_eq!(outgoing_comms.len(), 1);
    // Report mutations:
    assert_eq!(outgoing_control.len(), 1);

    // Node2 will send a RequestFunds message to Node1
    assert_eq!(outgoing_comms.len(), 1);
    let friend_message =
        if let FunderOutgoingComm::FriendMessage((_pk, friend_message)) = &outgoing_comms[0] {
            friend_message.clone()
        } else {
            unreachable!();
        };

    // Node1 receives RequestSendFunds from Node2:
    let funder_incoming =
        FunderIncoming::Comm(FunderIncomingComm::Friend((pk2.clone(), friend_message)));
    let (outgoing_comms, _outgoing_control) = Box::pin(apply_funder_incoming(
        funder_incoming,
        &mut state1,
        &mut ephemeral1,
        &mut rng,
        identity_client1,
    ))
    .await
    .unwrap();

    // Node1 sends a ResponseSendFunds to Node2:
    assert_eq!(outgoing_comms.len(), 1);
    let friend_message = match &outgoing_comms[0] {
        FunderOutgoingComm::FriendMessage((pk, friend_message)) => {
            if let FriendMessage::MoveTokenRequest(_move_token_request) = friend_message {
                assert_eq!(pk, &pk2);
            // let friend_move_token = &move_token_request.move_token;
            // assert_eq!(friend_move_token.balance, 0);
            // assert_eq!(friend_move_token.local_pending_debt, 0);
            // assert_eq!(friend_move_token.remote_pending_debt, 20);
            } else {
                unreachable!();
            }
            friend_message.clone()
        }
        _ => unreachable!(),
    };

    // Node2 receives ResponseSendFunds from Node1:
    let funder_incoming =
        FunderIncoming::Comm(FunderIncomingComm::Friend((pk1.clone(), friend_message)));
    let (_outgoing_comms, outgoing_control) = Box::pin(apply_funder_incoming(
        funder_incoming,
        &mut state2,
        &mut ephemeral2,
        &mut rng,
        identity_client2,
    ))
    .await
    .unwrap();

    assert_eq!(outgoing_control.len(), 2);
    let outgoing = &outgoing_control[1];
    let transaction_result = match outgoing {
        FunderOutgoingControl::TransactionResult(transaction_result) => transaction_result,
        _ => unreachable!(),
    };

    // We expect success:
    assert_eq!(transaction_result.request_id, Uid::from(&[1; Uid::len()]));
    let commit = match &transaction_result.result {
        RequestResult::Complete(commit) => commit.clone(),
        _ => unreachable!(),
    };

    // Node1: Apply Commit message received from Node2 (Received out of band):
    let incoming_control_message = FunderIncomingControl::new(
        Uid::from(&[21; Uid::len()]),
        FunderControl::CommitInvoice(commit),
    );
    let funder_incoming = FunderIncoming::Control(incoming_control_message);
    let (outgoing_comms, outgoing_control) = Box::pin(apply_funder_incoming(
        funder_incoming,
        &mut state1,
        &mut ephemeral1,
        &mut rng,
        identity_client1,
    ))
    .await
    .unwrap();

    assert_eq!(outgoing_comms.len(), 1);
    assert_eq!(outgoing_control.len(), 1);

    let friend_message = match &outgoing_comms[0] {
        FunderOutgoingComm::FriendMessage((pk, friend_message)) => {
            if let FriendMessage::MoveTokenRequest(_move_token_request) = friend_message {
                assert_eq!(pk, &pk2);
            // let friend_move_token = &move_token_request.move_token;
            // assert_eq!(friend_move_token.balance, 0);
            // assert_eq!(friend_move_token.local_pending_debt, 0);
            // assert_eq!(friend_move_token.remote_pending_debt, 20);
            } else {
                unreachable!();
            }
            friend_message.clone()
        }
        _ => unreachable!(),
    };

    // Node2 receives request token message from Node1:
    let funder_incoming =
        FunderIncoming::Comm(FunderIncomingComm::Friend((pk1.clone(), friend_message)));
    let (outgoing_comms, _outgoing_control) = Box::pin(apply_funder_incoming(
        funder_incoming,
        &mut state2,
        &mut ephemeral2,
        &mut rng,
        identity_client2,
    ))
    .await
    .unwrap();

    assert_eq!(outgoing_comms.len(), 1);

    // Node2 gives token to Node1:
    let friend_message = match &outgoing_comms[0] {
        FunderOutgoingComm::FriendMessage((pk, friend_message)) => {
            if let FriendMessage::MoveTokenRequest(_move_token_request) = friend_message {
                assert_eq!(pk, &pk1);
            // let friend_move_token = &move_token_request.move_token;
            // assert_eq!(friend_move_token.balance, 0);
            // assert_eq!(friend_move_token.local_pending_debt, 20);
            // assert_eq!(friend_move_token.remote_pending_debt, 0);
            } else {
                unreachable!();
            }
            friend_message.clone()
        }
        _ => unreachable!(),
    };

    // Node1 receives token from Node2:
    let funder_incoming =
        FunderIncoming::Comm(FunderIncomingComm::Friend((pk2.clone(), friend_message)));
    let (outgoing_comms, _outgoing_control) = Box::pin(apply_funder_incoming(
        funder_incoming,
        &mut state1,
        &mut ephemeral1,
        &mut rng,
        identity_client1,
    ))
    .await
    .unwrap();

    assert_eq!(outgoing_comms.len(), 1);

    // Node1 sends a Collect message to Node2:
    let friend_message = match &outgoing_comms[0] {
        FunderOutgoingComm::FriendMessage((pk, friend_message)) => {
            if let FriendMessage::MoveTokenRequest(_move_token_request) = friend_message {
                assert_eq!(pk, &pk2);
            // let friend_move_token = &move_token_request.move_token;
            // assert_eq!(friend_move_token.balance, 20);
            // assert_eq!(friend_move_token.local_pending_debt, 0);
            // assert_eq!(friend_move_token.remote_pending_debt, 0);
            } else {
                unreachable!();
            }
            friend_message.clone()
        }
        _ => unreachable!(),
    };

    // Node2 receives Collect message from node1:
    let funder_incoming =
        FunderIncoming::Comm(FunderIncomingComm::Friend((pk1.clone(), friend_message)));
    let (_outgoing_comms, _outgoing_control) = Box::pin(apply_funder_incoming(
        funder_incoming,
        &mut state2,
        &mut ephemeral2,
        &mut rng,
        identity_client2,
    ))
    .await
    .unwrap();

    // Current balance from Node1 point of view:
    let friend2 = state1.friends.get(&pk2).unwrap();
    let mutual_credit_state = match &friend2.channel_status {
        ChannelStatus::Consistent(channel_consistent) => channel_consistent
            .token_channel
            .get_mutual_credits()
            .get(&currency)
            .unwrap()
            .state(),
        _ => unreachable!(),
    };
    assert_eq!(mutual_credit_state.balance.balance, 20);
    assert_eq!(mutual_credit_state.balance.remote_pending_debt, 0);
    assert_eq!(mutual_credit_state.balance.local_pending_debt, 0);

    // Current balance from Node2 point of view:
    let friend1 = state2.friends.get(&pk1).unwrap();
    let mutual_credit_state = match &friend1.channel_status {
        ChannelStatus::Consistent(channel_consistent) => channel_consistent
            .token_channel
            .get_mutual_credits()
            .get(&currency)
            .unwrap()
            .state(),
        _ => unreachable!(),
    };
    assert_eq!(mutual_credit_state.balance.balance, -20);
    assert_eq!(mutual_credit_state.balance.remote_pending_debt, 0);
    assert_eq!(mutual_credit_state.balance.local_pending_debt, 0);

    // After a while...
    // Node2: Close the payment (To check the payment results)
    let incoming_control_message = FunderIncomingControl::new(
        Uid::from(&[21; Uid::len()]),
        FunderControl::RequestClosePayment(PaymentId::from(&[3u8; PaymentId::len()])),
    );
    let funder_incoming = FunderIncoming::Control(incoming_control_message);
    let (outgoing_comms, outgoing_control) = Box::pin(apply_funder_incoming(
        funder_incoming,
        &mut state2,
        &mut ephemeral2,
        &mut rng,
        identity_client2,
    ))
    .await
    .unwrap();

    assert_eq!(outgoing_comms.len(), 0);
    assert_eq!(outgoing_control.len(), 2);

    let response_close_payment = match &outgoing_control[1] {
        FunderOutgoingControl::ResponseClosePayment(response_close_payment) => {
            response_close_payment
        }
        _ => unreachable!(),
    };

    assert_eq!(
        response_close_payment.payment_id,
        PaymentId::from(&[3u8; PaymentId::len()])
    );
    let (receipt, ack_uid) = match &response_close_payment.status {
        PaymentStatus::Success(payment_status_success) => (
            payment_status_success.receipt.clone(),
            payment_status_success.ack_uid.clone(),
        ),
        _ => unreachable!(),
    };

    assert_eq!(
        receipt.invoice_id,
        InvoiceId::from(&[1u8; InvoiceId::len()])
    );
    assert_eq!(receipt.dest_payment, 16);
    assert_eq!(receipt.total_dest_payment, 16);

    let ack_close_payment = AckClosePayment {
        payment_id: PaymentId::from(&[3u8; PaymentId::len()]),
        ack_uid: ack_uid.clone(),
    };
    // Node2: Send ack for closing the payment:
    let incoming_control_message = FunderIncomingControl::new(
        Uid::from(&[22; Uid::len()]),
        FunderControl::AckClosePayment(ack_close_payment),
    );
    let funder_incoming = FunderIncoming::Control(incoming_control_message);
    let (_outgoing_comms, _outgoing_control) = Box::pin(apply_funder_incoming(
        funder_incoming,
        &mut state2,
        &mut ephemeral2,
        &mut rng,
        identity_client2,
    ))
    .await
    .unwrap();

    // Node2: Request for closing payment again:
    let incoming_control_message = FunderIncomingControl::new(
        Uid::from(&[23; Uid::len()]),
        FunderControl::RequestClosePayment(PaymentId::from(&[3u8; PaymentId::len()])),
    );
    let funder_incoming = FunderIncoming::Control(incoming_control_message);
    let (outgoing_comms, outgoing_control) = Box::pin(apply_funder_incoming(
        funder_incoming,
        &mut state2,
        &mut ephemeral2,
        &mut rng,
        identity_client2,
    ))
    .await
    .unwrap();

    assert_eq!(outgoing_comms.len(), 0);
    assert_eq!(outgoing_control.len(), 2);

    let response_close_payment = match &outgoing_control[1] {
        FunderOutgoingControl::ResponseClosePayment(response_close_payment) => {
            response_close_payment
        }
        _ => unreachable!(),
    };

    match response_close_payment.status {
        PaymentStatus::PaymentNotFound => {}
        _ => unreachable!(),
    }
}

#[test]
fn test_handler_pair_basic() {
    let thread_pool = ThreadPool::new().unwrap();

    let rng1 = DummyRandom::new(&[1u8]);
    let pkcs8 = PrivateKey::rand_gen(&rng1);
    let identity1 = SoftwareEd25519Identity::from_private_key(&pkcs8).unwrap();
    let (requests_sender1, identity_server1) = create_identity(identity1);
    let mut identity_client1 = IdentityClient::new(requests_sender1);
    thread_pool
        .spawn(identity_server1.then(|_| future::ready(())))
        .unwrap();

    let rng2 = DummyRandom::new(&[2u8]);
    let pkcs8 = PrivateKey::rand_gen(&rng2);
    let identity2 = SoftwareEd25519Identity::from_private_key(&pkcs8).unwrap();
    let (requests_sender2, identity_server2) = create_identity(identity2);
    let mut identity_client2 = IdentityClient::new(requests_sender2);
    thread_pool
        .spawn(identity_server2.then(|_| future::ready(())))
        .unwrap();

    LocalPool::new().run_until(task_handler_pair_basic(
        &mut identity_client1,
        &mut identity_client2,
    ));
}
