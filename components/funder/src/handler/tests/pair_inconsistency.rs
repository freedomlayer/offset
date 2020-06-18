use std::cmp::Ordering;
use std::convert::TryFrom;

use super::utils::{apply_funder_incoming, dummy_named_relay_address, dummy_relay_address};

use futures::executor::{LocalPool, ThreadPool};
use futures::task::SpawnExt;
use futures::{future, FutureExt};

use identity::{create_identity, IdentityClient};

use crypto::identity::{compare_public_key, SoftwareEd25519Identity};
use crypto::rand::RandGen;
use crypto::test_utils::DummyRandom;

use proto::crypto::{PrivateKey, Uid};
use proto::funder::messages::{
    AddFriend, Currency, CurrencyBalance, FriendMessage, FriendStatus, FunderControl,
    FunderIncomingControl, Rate, ResetFriendChannel, SetFriendCurrencyMaxDebt,
    SetFriendCurrencyRate, SetFriendStatus,
};

use crate::ephemeral::Ephemeral;
use crate::friend::{ChannelStatus, FriendMutation};
use crate::mutual_credit::types::McMutation;
use crate::state::{FunderMutation, FunderState};
use crate::token_channel::TcMutation;
use crate::types::{
    ChannelerConfig, FunderIncoming, FunderIncomingComm, FunderOutgoingComm,
    IncomingLivenessMessage,
};

async fn task_handler_pair_inconsistency<'a>(
    identity_client1: &'a mut IdentityClient,
    identity_client2: &'a mut IdentityClient,
) {
    // NOTE: We use Box::pin() in order to make sure we don't get a too large Future which will
    // cause a stack overflow.
    // See:  https://github.com/rust-lang-nursery/futures-rs/issues/1330

    let currency = Currency::try_from("FST".to_owned()).unwrap();
    let currency2 = Currency::try_from("FST2".to_owned()).unwrap();
    let currency3 = Currency::try_from("FST3".to_owned()).unwrap();

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

    let mut rng = DummyRandom::new(&[3u8]);

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

    ///////////////////////////////////////
    // Set remote max debt (Node1)
    ///////////////////////////////////////

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

    // Node1 sends nothing:
    assert!(outgoing_comms.is_empty());

    //////////////////////////////////////////
    // Add currency: (Node1 -> Node2):
    //////////////////////////////////////////

    // Node1 receives control message to add another currency:
    let set_friend_currency_rate = SetFriendCurrencyRate {
        friend_public_key: pk2.clone(),
        currency: currency2.clone(),
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

    /////////////////////////////////////////////////
    // Create inconsistency intentionaly:
    /////////////////////////////////////////////////

    // Change the balance intentionaly, to cause an inconsistency error.
    // Node1 will now think that Node2 owes him 10 credits:
    let mc_mutation = McMutation::SetBalance(10i128);
    let tc_mutation = TcMutation::McMutation((currency.clone(), mc_mutation));
    let friend_mutation = FriendMutation::TcMutation(tc_mutation);
    let funder_mutation = FunderMutation::FriendMutation((pk2.clone(), friend_mutation));
    state1.mutate(&funder_mutation);

    ///////////////////////////////////////
    // Add currency3 (Node2 -> Node1)
    // This will not work, because an inconsistency will be noticed
    ///////////////////////////////////////

    // Node2 receives control message to add another currency:
    let set_friend_currency_rate = SetFriendCurrencyRate {
        friend_public_key: pk1.clone(),
        currency: currency3.clone(),
        rate: Rate::new(),
    };
    let incoming_control_message = FunderIncomingControl::new(
        Uid::from(&[19; Uid::len()]),
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
    let (outgoing_comms, _outgoing_control) = Box::pin(apply_funder_incoming(
        funder_incoming,
        &mut state1,
        &mut ephemeral1,
        &mut rng,
        identity_client1,
    ))
    .await
    .unwrap();

    /////////////////////////////////////////////////
    // Inconsistency is noticed:
    /////////////////////////////////////////////////

    let friend_message = match &outgoing_comms[0] {
        FunderOutgoingComm::FriendMessage((pk, friend_message)) => {
            if let FriendMessage::InconsistencyError(reset_terms) = friend_message {
                assert_eq!(reset_terms.inconsistency_counter, 1);
                assert_eq!(
                    reset_terms.balance_for_reset,
                    vec![CurrencyBalance {
                        currency: currency.clone(),
                        balance: 10i128
                    }]
                );
                assert_eq!(pk, &pk2);
            } else {
                unreachable!();
            }
            friend_message.clone()
        }
        _ => unreachable!(),
    };

    // Node2: Receive InconsistencyError from Node1:
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

    // Node2 should send his reset terms:
    assert_eq!(outgoing_comms.len(), 1);

    let (friend_message, reset_token2) = match &outgoing_comms[0] {
        FunderOutgoingComm::FriendMessage((pk, friend_message)) => {
            if let FriendMessage::InconsistencyError(reset_terms) = friend_message {
                assert_eq!(reset_terms.inconsistency_counter, 1);
                assert_eq!(
                    reset_terms.balance_for_reset,
                    vec![CurrencyBalance {
                        currency: currency.clone(),
                        balance: 0
                    }]
                );
                assert_eq!(pk, &pk1);
                (friend_message.clone(), reset_terms.reset_token.clone())
            } else {
                unreachable!();
            }
        }
        _ => unreachable!(),
    };

    // Node1: Receive InconsistencyError from Node2:
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

    // Resolving the inconsistency
    // ---------------------------

    // Node1: Reset channel, agreeing to Node2's conditions:
    let reset_friend_channel = ResetFriendChannel {
        friend_public_key: pk2.clone(),
        reset_token: reset_token2.clone(),
    };
    let incoming_control_message = FunderIncomingControl::new(
        Uid::from(&[15; Uid::len()]),
        FunderControl::ResetFriendChannel(reset_friend_channel),
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

    let friend2 = state1.friends.get(&pk2).unwrap();
    match &friend2.channel_status {
        ChannelStatus::Consistent(channel_consistent) => {
            assert_eq!(
                channel_consistent
                    .token_channel
                    .get_mutual_credits()
                    .get(&currency)
                    .unwrap()
                    .state()
                    .balance
                    .balance,
                0i128
            );
        }
        _ => unreachable!(),
    };

    // Node1 should send a MoveToken message that resolves the inconsistency:
    assert_eq!(outgoing_comms.len(), 1);
    let friend_message = match &outgoing_comms[0] {
        FunderOutgoingComm::FriendMessage((pk, friend_message)) => {
            if let FriendMessage::MoveTokenRequest(move_token_request) = friend_message {
                assert_eq!(pk, &pk2);
                // Token is wanted because local relays were sent:
                assert_eq!(move_token_request.token_wanted, true);

                let friend_move_token = &move_token_request.move_token;
                assert_eq!(friend_move_token.old_token, reset_token2);
                assert!(friend_move_token.opt_local_relays.is_some());
            } else {
                unreachable!();
            }
            friend_message.clone()
        }
        _ => unreachable!(),
    };

    // Node2: Receive MoveToken (that resolves inconsistency) from Node1:
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

    // Node2 should send back a move token with local relays:
    assert_eq!(outgoing_comms.len(), 2);
    let friend_message = match &outgoing_comms[1] {
        FunderOutgoingComm::FriendMessage((pk, friend_message)) => {
            if let FriendMessage::MoveTokenRequest(move_token_request) = friend_message {
                assert_eq!(pk, &pk1);
                // Token is wanted because local relays were sent:
                assert_eq!(move_token_request.token_wanted, true);

                let friend_move_token = &move_token_request.move_token;
                assert!(friend_move_token.currencies_operations.is_empty());
                assert!(friend_move_token.opt_local_relays.is_some());
            } else {
                unreachable!();
            }
            friend_message.clone()
        }
        _ => unreachable!(),
    };

    // Node1: Receive MoveToken from Node2:
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

    // Node1 sends to Node2 an empty move token message (Because token is wanted by Node2):
    assert_eq!(outgoing_comms.len(), 2);
    let friend_message = match &outgoing_comms[1] {
        FunderOutgoingComm::FriendMessage((pk, friend_message)) => {
            if let FriendMessage::MoveTokenRequest(move_token_request) = friend_message {
                assert_eq!(pk, &pk2);
                assert_eq!(move_token_request.token_wanted, false);

                let friend_move_token = &move_token_request.move_token;
                assert!(friend_move_token.currencies_operations.is_empty());
                assert!(friend_move_token.opt_local_relays.is_none());
            } else {
                unreachable!();
            }
            friend_message.clone()
        }
        _ => unreachable!(),
    };

    // Node2: Receive empty MoveToken from Node1:
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
}

#[test]
fn test_handler_pair_inconsistency() {
    let thread_pool = ThreadPool::new().unwrap();

    let mut rng1 = DummyRandom::new(&[1u8]);
    let pkcs8 = PrivateKey::rand_gen(&mut rng1);
    let identity1 = SoftwareEd25519Identity::from_private_key(&pkcs8).unwrap();
    let (requests_sender1, identity_server1) = create_identity(identity1);
    let mut identity_client1 = IdentityClient::new(requests_sender1);
    thread_pool
        .spawn(identity_server1.then(|_| future::ready(())))
        .unwrap();

    let mut rng2 = DummyRandom::new(&[2u8]);
    let pkcs8 = PrivateKey::rand_gen(&mut rng2);
    let identity2 = SoftwareEd25519Identity::from_private_key(&pkcs8).unwrap();
    let (requests_sender2, identity_server2) = create_identity(identity2);
    let mut identity_client2 = IdentityClient::new(requests_sender2);
    thread_pool
        .spawn(identity_server2.then(|_| future::ready(())))
        .unwrap();

    LocalPool::new().run_until(task_handler_pair_inconsistency(
        &mut identity_client1,
        &mut identity_client2,
    ));
}
