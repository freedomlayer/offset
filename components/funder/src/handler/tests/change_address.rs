use super::utils::apply_funder_incoming;

use std::cmp::Ordering;

use futures::executor::{ThreadPool, LocalPool};
use futures::task::SpawnExt;
use futures::{future, FutureExt};

use identity::{create_identity, IdentityClient};

use crypto::identity::{compare_public_key, generate_private_key, SoftwareEd25519Identity};
use crypto::rand::RngContainer;
use crypto::test_utils::DummyRandom;

use proto::crypto::Uid;

use proto::funder::messages::{
    AddFriend, FriendMessage, FriendStatus, FunderControl, FunderIncomingControl, SetFriendStatus,
};

use crate::ephemeral::Ephemeral;
use crate::state::FunderState;
use crate::tests::utils::{dummy_named_relay_address, dummy_relay_address};
use crate::types::{
    ChannelerConfig, FunderIncoming, FunderIncomingComm, FunderOutgoingComm,
    IncomingLivenessMessage,
};

async fn task_handler_change_address(
    identity_client1: IdentityClient,
    identity_client2: IdentityClient,
) {
    // NOTE: We use Box::pin() in order to make sure we don't get a too large Future which will
    // cause a stack overflow.
    // See:  https://github.com/rust-lang-nursery/futures-rs/issues/1330

    // Sort the identities. identity_client1 will be the first sender:
    let pk1 = identity_client1.request_public_key().await.unwrap();
    let pk2 = identity_client2.request_public_key().await.unwrap();
    let (mut identity_client1, pk1, mut identity_client2, pk2) =
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
        &mut identity_client1,
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
        &mut identity_client2,
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
    apply_funder_incoming(
        funder_incoming,
        &mut state1,
        &mut ephemeral1,
        &mut rng,
        &mut identity_client1,
    )
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
        &mut identity_client1,
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
        &mut identity_client2,
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
        &mut identity_client2,
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
        &mut identity_client1,
    ))
    .await
    .unwrap();

    assert_eq!(outgoing_comms.len(), 1);
    let friend_message = match &outgoing_comms[0] {
        FunderOutgoingComm::FriendMessage((pk, friend_message)) => {
            if let FriendMessage::MoveTokenRequest(move_token_request) = friend_message {
                assert_eq!(pk, &pk2);
                assert_eq!(move_token_request.token_wanted, true);

                let friend_move_token = &move_token_request.move_token;
                // assert_eq!(friend_move_token.move_token_counter, 0);
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

    // Node2: Notify that Node1 is alive
    let incoming_liveness_message = IncomingLivenessMessage::Online(pk1.clone());
    let funder_incoming =
        FunderIncoming::Comm(FunderIncomingComm::Liveness(incoming_liveness_message));
    Box::pin(apply_funder_incoming(
        funder_incoming,
        &mut state2,
        &mut ephemeral2,
        &mut rng,
        &mut identity_client2,
    ))
    .await
    .unwrap();

    // Node2: Receive friend_message from Node1:
    let funder_incoming =
        FunderIncoming::Comm(FunderIncomingComm::Friend((pk1.clone(), friend_message)));
    let (outgoing_comms, _outgoing_control) = Box::pin(apply_funder_incoming(
        funder_incoming,
        &mut state2,
        &mut ephemeral2,
        &mut rng,
        &mut identity_client2,
    ))
    .await
    .unwrap();

    assert_eq!(outgoing_comms.len(), 1);
    let friend_message = match &outgoing_comms[0] {
        FunderOutgoingComm::FriendMessage((pk, friend_message)) => {
            if let FriendMessage::MoveTokenRequest(move_token_request) = friend_message {
                assert_eq!(pk, &pk1);
                assert_eq!(move_token_request.token_wanted, true);
                let friend_move_token = &move_token_request.move_token;
                assert!(friend_move_token.currencies_operations.is_empty());

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

    // Node1: Receive friend_message from Node2:
    let funder_incoming =
        FunderIncoming::Comm(FunderIncomingComm::Friend((pk2.clone(), friend_message)));
    let (outgoing_comms, _outgoing_control) = Box::pin(apply_funder_incoming(
        funder_incoming,
        &mut state1,
        &mut ephemeral1,
        &mut rng,
        &mut identity_client1,
    ))
    .await
    .unwrap();

    assert_eq!(outgoing_comms.len(), 2);
    let friend_message = match &outgoing_comms[1] {
        FunderOutgoingComm::FriendMessage((pk, friend_message)) => {
            if let FriendMessage::MoveTokenRequest(move_token_request) = friend_message {
                assert_eq!(pk, &pk2);
                assert_eq!(move_token_request.token_wanted, true);
                let friend_move_token = &move_token_request.move_token;
                assert!(friend_move_token.currencies_operations.is_empty());

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

    // Node2: Receive friend_message from Node1:
    let funder_incoming =
        FunderIncoming::Comm(FunderIncomingComm::Friend((pk1.clone(), friend_message)));
    let (outgoing_comms, _outgoing_control) = Box::pin(apply_funder_incoming(
        funder_incoming,
        &mut state2,
        &mut ephemeral2,
        &mut rng,
        &mut identity_client2,
    ))
    .await
    .unwrap();

    assert_eq!(outgoing_comms.len(), 1);
    let friend_message = match &outgoing_comms[0] {
        FunderOutgoingComm::FriendMessage((pk, friend_message)) => {
            if let FriendMessage::MoveTokenRequest(move_token_request) = friend_message {
                assert_eq!(pk, &pk1);
                assert_eq!(move_token_request.token_wanted, false);
                let friend_move_token = &move_token_request.move_token;
                assert!(friend_move_token.currencies_operations.is_empty());

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

    // Node1: Receive friend_message from Node2:
    let funder_incoming =
        FunderIncoming::Comm(FunderIncomingComm::Friend((pk2.clone(), friend_message)));
    let (outgoing_comms, _outgoing_control) = Box::pin(apply_funder_incoming(
        funder_incoming,
        &mut state1,
        &mut ephemeral1,
        &mut rng,
        &mut identity_client1,
    ))
    .await
    .unwrap();
    assert!(outgoing_comms.is_empty());

    // Node1 decides to change his address:
    let incoming_control_message = FunderIncomingControl::new(
        Uid::from(&[15; Uid::len()]),
        FunderControl::AddRelay(dummy_named_relay_address(11)),
    );
    let funder_incoming = FunderIncoming::Control(incoming_control_message);
    let (outgoing_comms, _outgoing_control) = apply_funder_incoming(
        funder_incoming,
        &mut state1,
        &mut ephemeral1,
        &mut rng,
        &mut identity_client1,
    )
    .await
    .unwrap();

    // Node1 sends an update about his new address:
    assert_eq!(outgoing_comms.len(), 3);

    match &outgoing_comms[0] {
        FunderOutgoingComm::ChannelerConfig(ChannelerConfig::SetRelays(relays)) => {
            assert_eq!(
                relays,
                &vec![dummy_relay_address(1), dummy_relay_address(11)]
            );
        }
        _ => unreachable!(),
    };

    match &outgoing_comms[1] {
        FunderOutgoingComm::ChannelerConfig(ChannelerConfig::UpdateFriend(update_friend)) => {
            assert_eq!(update_friend.friend_public_key, pk2);
            assert_eq!(update_friend.friend_relays, vec![dummy_relay_address(2)]);
            assert_eq!(
                update_friend.local_relays,
                vec![dummy_relay_address(1), dummy_relay_address(11)]
            );
        }
        _ => unreachable!(),
    };

    let friend_message = match &outgoing_comms[2] {
        FunderOutgoingComm::FriendMessage((pk, friend_message)) => {
            if let FriendMessage::MoveTokenRequest(move_token_request) = friend_message {
                assert_eq!(pk, &pk2);
                assert_eq!(move_token_request.token_wanted, true);
                let friend_move_token = &move_token_request.move_token;
                assert!(friend_move_token.currencies_operations.is_empty());

                // assert_eq!(friend_move_token.move_token_counter, 4);
                // assert_eq!(friend_move_token.inconsistency_counter, 0);
                // assert_eq!(friend_move_token.balance, 0);
                let expected_address = vec![dummy_relay_address(1), dummy_relay_address(11)];
                assert_eq!(friend_move_token.opt_local_relays, Some(expected_address));
            } else {
                unreachable!();
            }
            friend_message.clone()
        }
        _ => unreachable!(),
    };

    // Node2: Receive friend_message from Node1:
    let funder_incoming =
        FunderIncoming::Comm(FunderIncomingComm::Friend((pk1.clone(), friend_message)));
    let (outgoing_comms, _outgoing_control) = Box::pin(apply_funder_incoming(
        funder_incoming,
        &mut state2,
        &mut ephemeral2,
        &mut rng,
        &mut identity_client2,
    ))
    .await
    .unwrap();

    assert_eq!(outgoing_comms.len(), 1);
    let friend_message = match &outgoing_comms[0] {
        FunderOutgoingComm::FriendMessage((pk, friend_message)) => {
            if let FriendMessage::MoveTokenRequest(move_token_request) = friend_message {
                assert_eq!(pk, &pk1);
                assert_eq!(move_token_request.token_wanted, false);
                let friend_move_token = &move_token_request.move_token;
                assert!(friend_move_token.currencies_operations.is_empty());

                // assert_eq!(friend_move_token.move_token_counter, 5);
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

    // Node1: Receive friend_message from Node2:
    let funder_incoming =
        FunderIncoming::Comm(FunderIncomingComm::Friend((pk2.clone(), friend_message)));
    let (outgoing_comms, _outgoing_control) = Box::pin(apply_funder_incoming(
        funder_incoming,
        &mut state1,
        &mut ephemeral1,
        &mut rng,
        &mut identity_client1,
    ))
    .await
    .unwrap();
    assert_eq!(outgoing_comms.len(), 1);
    match &outgoing_comms[0] {
        FunderOutgoingComm::ChannelerConfig(ChannelerConfig::UpdateFriend(update_friend)) => {
            assert_eq!(update_friend.friend_public_key, pk2);
            assert_eq!(update_friend.friend_relays, vec![dummy_relay_address(2)]);
            assert_eq!(
                update_friend.local_relays,
                vec![dummy_relay_address(1), dummy_relay_address(11)]
            );
        }
        _ => unreachable!(),
    };
}

#[test]
fn test_handler_change_address() {
    let thread_pool = ThreadPool::new().unwrap();

    let rng1 = DummyRandom::new(&[1u8]);
    let pkcs8 = generate_private_key(&rng1);
    let identity1 = SoftwareEd25519Identity::from_private_key(&pkcs8).unwrap();
    let (requests_sender1, identity_server1) = create_identity(identity1);
    let identity_client1 = IdentityClient::new(requests_sender1);
    thread_pool
        .spawn(identity_server1.then(|_| future::ready(())))
        .unwrap();

    let rng2 = DummyRandom::new(&[2u8]);
    let pkcs8 = generate_private_key(&rng2);
    let identity2 = SoftwareEd25519Identity::from_private_key(&pkcs8).unwrap();
    let (requests_sender2, identity_server2) = create_identity(identity2);
    let identity_client2 = IdentityClient::new(requests_sender2);
    thread_pool
        .spawn(identity_server2.then(|_| future::ready(())))
        .unwrap();

    LocalPool::new().run_until(task_handler_change_address(
        identity_client1,
        identity_client2,
    ));
}
