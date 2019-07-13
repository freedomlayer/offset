use super::utils::apply_funder_incoming;

use std::cmp::Ordering;

use futures::executor::ThreadPool;
use futures::task::SpawnExt;
use futures::{future, FutureExt};

use identity::{create_identity, IdentityClient};

use crypto::identity::{compare_public_key, generate_private_key, SoftwareEd25519Identity};
use crypto::rand::RngContainer;
use crypto::test_utils::DummyRandom;


use proto::crypto::{Uid, UID_LEN};
use proto::funder::messages::{
    AddFriend, FriendMessage, FriendStatus, FunderControl, FunderIncomingControl,
    ResetFriendChannel, SetFriendStatus,
};

use crate::ephemeral::Ephemeral;
use crate::friend::ChannelStatus;
use crate::state::FunderState;
use crate::types::{
    FunderIncoming, FunderIncomingComm, FunderOutgoingComm, IncomingLivenessMessage,
};

use crate::tests::utils::{dummy_named_relay_address, dummy_relay_address};

async fn task_handler_pair_inconsistency<'a>(
    identity_client1: &'a mut IdentityClient,
    identity_client2: &'a mut IdentityClient,
) {
    // NOTE: We use Box::pin() in order to make sure we don't get a too large Future which will
    // cause a stack overflow.
    // See:  https://github.com/rust-lang-nursery/futures-rs/issues/1330

    // Sort the identities. identity_client1 will be the first sender:
    let pk1 = await!(identity_client1.request_public_key()).unwrap();
    let pk2 = await!(identity_client2.request_public_key()).unwrap();
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
    await!(Box::pin(apply_funder_incoming(
        funder_incoming,
        &mut state1,
        &mut ephemeral1,
        &mut rng,
        identity_client1
    )))
    .unwrap();

    // Initialize 2:
    let funder_incoming = FunderIncoming::Init;
    await!(Box::pin(apply_funder_incoming(
        funder_incoming,
        &mut state2,
        &mut ephemeral2,
        &mut rng,
        identity_client2
    )))
    .unwrap();

    // Node1: Add friend 2:
    let add_friend = AddFriend {
        friend_public_key: pk2.clone(),
        relays: vec![dummy_relay_address(2)],
        name: String::from("pk2"),
        balance: 20i128,
    };
    let incoming_control_message = FunderIncomingControl::new(
        Uid::from(&[11; UID_LEN]),
        FunderControl::AddFriend(add_friend),
    );
    let funder_incoming = FunderIncoming::Control(incoming_control_message);
    await!(Box::pin(apply_funder_incoming(
        funder_incoming,
        &mut state1,
        &mut ephemeral1,
        &mut rng,
        identity_client1
    )))
    .unwrap();

    // Node1: Enable friend 2:
    let set_friend_status = SetFriendStatus {
        friend_public_key: pk2.clone(),
        status: FriendStatus::Enabled,
    };
    let incoming_control_message = FunderIncomingControl::new(
        Uid::from(&[12; UID_LEN]),
        FunderControl::SetFriendStatus(set_friend_status),
    );
    let funder_incoming = FunderIncoming::Control(incoming_control_message);
    await!(Box::pin(apply_funder_incoming(
        funder_incoming,
        &mut state1,
        &mut ephemeral1,
        &mut rng,
        identity_client1
    )))
    .unwrap();

    // Node2: Add friend 1:
    // Note that this friend's initial balance should have been
    // -20i128, but we assign -10i128 to cause an inconsistency.
    let add_friend = AddFriend {
        friend_public_key: pk1.clone(),
        relays: vec![dummy_relay_address(1)],
        name: String::from("pk1"),
        balance: -10i128,
    };
    let incoming_control_message = FunderIncomingControl::new(
        Uid::from(&[13; UID_LEN]),
        FunderControl::AddFriend(add_friend),
    );
    let funder_incoming = FunderIncoming::Control(incoming_control_message);
    await!(Box::pin(apply_funder_incoming(
        funder_incoming,
        &mut state2,
        &mut ephemeral2,
        &mut rng,
        identity_client2
    )))
    .unwrap();

    // Node2: enable friend 1:
    let set_friend_status = SetFriendStatus {
        friend_public_key: pk1.clone(),
        status: FriendStatus::Enabled,
    };
    let incoming_control_message = FunderIncomingControl::new(
        Uid::from(&[14; UID_LEN]),
        FunderControl::SetFriendStatus(set_friend_status),
    );
    let funder_incoming = FunderIncoming::Control(incoming_control_message);
    await!(Box::pin(apply_funder_incoming(
        funder_incoming,
        &mut state2,
        &mut ephemeral2,
        &mut rng,
        identity_client2
    )))
    .unwrap();

    // Node1: Notify that Node2 is alive
    // We expect that Node1 will resend his outgoing message when he is notified that Node1 is online.
    let incoming_liveness_message = IncomingLivenessMessage::Online(pk2.clone());
    let funder_incoming =
        FunderIncoming::Comm(FunderIncomingComm::Liveness(incoming_liveness_message));
    let (outgoing_comms, _outgoing_control) = await!(Box::pin(apply_funder_incoming(
        funder_incoming,
        &mut state1,
        &mut ephemeral1,
        &mut rng,
        identity_client1
    )))
    .unwrap();

    assert_eq!(outgoing_comms.len(), 1);
    let friend_message = match &outgoing_comms[0] {
        FunderOutgoingComm::FriendMessage((pk, friend_message)) => {
            if let FriendMessage::MoveTokenRequest(move_token_request) = friend_message {
                assert_eq!(pk, &pk2);
                // Token is wanted because Node1 wants to send his configured address later.
                assert_eq!(move_token_request.token_wanted, true);

                let friend_move_token = &move_token_request.move_token;
                assert_eq!(friend_move_token.move_token_counter, 0);
                assert_eq!(friend_move_token.inconsistency_counter, 0);
                assert_eq!(friend_move_token.balance, 20i128);
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
    // TODO: Check outgoing_comms here:
    let (outgoing_comms, _outgoing_control) = await!(Box::pin(apply_funder_incoming(
        funder_incoming,
        &mut state2,
        &mut ephemeral2,
        &mut rng,
        identity_client2
    )))
    .unwrap();

    // Node2 sends information about his address to Node1:
    assert_eq!(outgoing_comms.len(), 2);

    // Node2: Receive MoveToken from Node1:
    let funder_incoming =
        FunderIncoming::Comm(FunderIncomingComm::Friend((pk1.clone(), friend_message)));
    let (outgoing_comms, _outgoing_control) = await!(Box::pin(apply_funder_incoming(
        funder_incoming,
        &mut state2,
        &mut ephemeral2,
        &mut rng,
        identity_client2
    )))
    .unwrap();

    // Node2 should retransmit his outgoing message:
    // Note that Node2 haven't yet detected the inconsistency.
    assert_eq!(outgoing_comms.len(), 1);

    let friend_message = match &outgoing_comms[0] {
        FunderOutgoingComm::FriendMessage((pk, friend_message)) => {
            if let FriendMessage::MoveTokenRequest(move_token_request) = friend_message {
                assert_eq!(pk, &pk1);
                assert_eq!(move_token_request.token_wanted, true);

                let friend_move_token = &move_token_request.move_token;
                assert_eq!(friend_move_token.move_token_counter, 1);
                assert_eq!(friend_move_token.inconsistency_counter, 0);
                assert_eq!(friend_move_token.balance, -10i128);
                assert!(friend_move_token.opt_local_relays.is_some());
            } else {
                unreachable!();
            }
            friend_message.clone()
        }
        _ => unreachable!(),
    };

    // Node1: Receive MoveToken from Node2 with invalid balance.
    // At this point Node1 should detect inconsistency
    let funder_incoming =
        FunderIncoming::Comm(FunderIncomingComm::Friend((pk2.clone(), friend_message)));
    let (outgoing_comms, _outgoing_control) = await!(Box::pin(apply_funder_incoming(
        funder_incoming,
        &mut state1,
        &mut ephemeral1,
        &mut rng,
        identity_client1
    )))
    .unwrap();

    // Node1 should send an inconsistency error:
    assert_eq!(outgoing_comms.len(), 1);

    let friend_message = match &outgoing_comms[0] {
        FunderOutgoingComm::FriendMessage((pk, friend_message)) => {
            if let FriendMessage::InconsistencyError(reset_terms) = friend_message {
                assert_eq!(reset_terms.inconsistency_counter, 1);
                assert_eq!(reset_terms.balance_for_reset, 20i128);
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
    let (outgoing_comms, _outgoing_control) = await!(Box::pin(apply_funder_incoming(
        funder_incoming,
        &mut state2,
        &mut ephemeral2,
        &mut rng,
        identity_client2
    )))
    .unwrap();

    // Node2 should send his reset terms:
    assert_eq!(outgoing_comms.len(), 1);

    let (friend_message, reset_token2) = match &outgoing_comms[0] {
        FunderOutgoingComm::FriendMessage((pk, friend_message)) => {
            if let FriendMessage::InconsistencyError(reset_terms) = friend_message {
                assert_eq!(reset_terms.inconsistency_counter, 1);
                assert_eq!(reset_terms.balance_for_reset, -10i128);
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
    let (outgoing_comms, _outgoing_control) = await!(Box::pin(apply_funder_incoming(
        funder_incoming,
        &mut state1,
        &mut ephemeral1,
        &mut rng,
        identity_client1
    )))
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
        Uid::from(&[15; UID_LEN]),
        FunderControl::ResetFriendChannel(reset_friend_channel),
    );
    let funder_incoming = FunderIncoming::Control(incoming_control_message);
    let (outgoing_comms, _outgoing_control) = await!(Box::pin(apply_funder_incoming(
        funder_incoming,
        &mut state1,
        &mut ephemeral1,
        &mut rng,
        identity_client1
    )))
    .unwrap();

    let friend2 = state1.friends.get(&pk2).unwrap();
    match &friend2.channel_status {
        ChannelStatus::Consistent(channel_consistent) => {
            assert_eq!(
                channel_consistent
                    .token_channel
                    .get_mutual_credit()
                    .state()
                    .balance
                    .balance,
                10i128
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
                // Token is wanted because Node1 wants to send his configured address later.
                assert_eq!(move_token_request.token_wanted, true);

                let friend_move_token = &move_token_request.move_token;
                assert_eq!(friend_move_token.old_token, reset_token2);
                assert_eq!(friend_move_token.move_token_counter, 0);
                assert_eq!(friend_move_token.inconsistency_counter, 1);
                assert_eq!(friend_move_token.balance, 10i128);
                assert!(friend_move_token.opt_local_relays.is_none());
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
    let (outgoing_comms, _outgoing_control) = await!(Box::pin(apply_funder_incoming(
        funder_incoming,
        &mut state2,
        &mut ephemeral2,
        &mut rng,
        identity_client2
    )))
    .unwrap();

    // Node2 should send back an empty move token:
    assert_eq!(outgoing_comms.len(), 1);
    let friend_message = match &outgoing_comms[0] {
        FunderOutgoingComm::FriendMessage((pk, friend_message)) => {
            if let FriendMessage::MoveTokenRequest(move_token_request) = friend_message {
                assert_eq!(pk, &pk1);
                // Token is wanted because Node2 wants to send his configured address later.
                assert_eq!(move_token_request.token_wanted, false);

                let friend_move_token = &move_token_request.move_token;
                assert!(friend_move_token.operations.is_empty());
                assert_eq!(friend_move_token.move_token_counter, 1);
                assert_eq!(friend_move_token.inconsistency_counter, 1);
                assert_eq!(friend_move_token.balance, -10i128);
                assert!(friend_move_token.opt_local_relays.is_none());
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
    let (outgoing_comms, _outgoing_control) = await!(Box::pin(apply_funder_incoming(
        funder_incoming,
        &mut state1,
        &mut ephemeral1,
        &mut rng,
        identity_client1
    )))
    .unwrap();

    // Inconsistency is resolved.
    // Node1 sends his address:
    assert_eq!(outgoing_comms.len(), 2);
    let friend_message = match &outgoing_comms[1] {
        FunderOutgoingComm::FriendMessage((pk, friend_message)) => {
            if let FriendMessage::MoveTokenRequest(move_token_request) = friend_message {
                assert_eq!(pk, &pk2);
                assert_eq!(move_token_request.token_wanted, true);

                let friend_move_token = &move_token_request.move_token;
                assert!(friend_move_token.operations.is_empty());
                assert_eq!(friend_move_token.move_token_counter, 2);
                assert_eq!(friend_move_token.inconsistency_counter, 1);
                assert_eq!(friend_move_token.balance, 10i128);
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

    // Node2: Receive MoveToken from Node1:
    let funder_incoming =
        FunderIncoming::Comm(FunderIncomingComm::Friend((pk1.clone(), friend_message)));
    let (outgoing_comms, _outgoing_control) = await!(Box::pin(apply_funder_incoming(
        funder_incoming,
        &mut state2,
        &mut ephemeral2,
        &mut rng,
        identity_client2
    )))
    .unwrap();

    assert_eq!(outgoing_comms.len(), 1);
    let friend_message = match &outgoing_comms[0] {
        FunderOutgoingComm::FriendMessage((pk, friend_message)) => {
            if let FriendMessage::MoveTokenRequest(move_token_request) = friend_message {
                assert_eq!(pk, &pk1);
                assert_eq!(move_token_request.token_wanted, false);

                let friend_move_token = &move_token_request.move_token;
                assert!(friend_move_token.operations.is_empty());
                assert_eq!(friend_move_token.move_token_counter, 3);
                assert_eq!(friend_move_token.inconsistency_counter, 1);
                assert_eq!(friend_move_token.balance, -10i128);
                assert_eq!(friend_move_token.opt_local_relays, None);
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
    let (outgoing_comms, _outgoing_control) = await!(Box::pin(apply_funder_incoming(
        funder_incoming,
        &mut state1,
        &mut ephemeral1,
        &mut rng,
        identity_client1
    )))
    .unwrap();
    assert!(outgoing_comms.is_empty());
}

#[test]
fn test_handler_pair_inconsistency() {
    let mut thread_pool = ThreadPool::new().unwrap();

    let rng1 = DummyRandom::new(&[1u8]);
    let pkcs8 = generate_private_key(&rng1);
    let identity1 = SoftwareEd25519Identity::from_private_key(&pkcs8).unwrap();
    let (requests_sender1, identity_server1) = create_identity(identity1);
    let mut identity_client1 = IdentityClient::new(requests_sender1);
    thread_pool
        .spawn(identity_server1.then(|_| future::ready(())))
        .unwrap();

    let rng2 = DummyRandom::new(&[2u8]);
    let pkcs8 = generate_private_key(&rng2);
    let identity2 = SoftwareEd25519Identity::from_private_key(&pkcs8).unwrap();
    let (requests_sender2, identity_server2) = create_identity(identity2);
    let mut identity_client2 = IdentityClient::new(requests_sender2);
    thread_pool
        .spawn(identity_server2.then(|_| future::ready(())))
        .unwrap();

    thread_pool.run(task_handler_pair_inconsistency(
        &mut identity_client1,
        &mut identity_client2,
    ));
}
