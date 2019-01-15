use super::utils::apply_funder_incoming;

use std::cmp::Ordering;

use futures::executor::ThreadPool;
use futures::{future, FutureExt};
use futures::task::SpawnExt;

use identity::{create_identity, IdentityClient};

use crypto::test_utils::DummyRandom;
use crypto::identity::{SoftwareEd25519Identity, generate_pkcs8_key_pair, compare_public_key};
use crypto::crypto_rand::RngContainer;


use proto::funder::messages::{FriendMessage, 
    FunderIncomingControl, 
    AddFriend, FriendStatus,
    SetFriendStatus};

use crate::types::{FunderIncoming, IncomingLivenessMessage, 
    FunderOutgoingComm, FunderIncomingComm, ChannelerConfig};
use crate::ephemeral::Ephemeral;
use crate::state::FunderState;

async fn task_handler_change_address(identity_client1: IdentityClient, 
                                 identity_client2: IdentityClient) {
    // NOTE: We use Box::pin() in order to make sure we don't get a too large Future which will
    // cause a stack overflow. 
    // See:  https://github.com/rust-lang-nursery/futures-rs/issues/1330 
    
    // Sort the identities. identity_client1 will be the first sender:
    let pk1 = await!(identity_client1.request_public_key()).unwrap();
    let pk2 = await!(identity_client2.request_public_key()).unwrap();
    let (mut identity_client1, pk1, mut identity_client2, pk2) = if compare_public_key(&pk1, &pk2) == Ordering::Less {
        (identity_client1, pk1, identity_client2, pk2)
    } else {
        (identity_client2, pk2, identity_client1, pk1)
    };

    let mut state1 = FunderState::<u32>::new(&pk1, Some(&0x1337u32));
    let mut ephemeral1 = Ephemeral::new(&state1);
    let mut state2 = FunderState::<u32>::new(&pk2, Some(&0x1338u32));
    let mut ephemeral2 = Ephemeral::new(&state2);

    let mut rng = RngContainer::new(DummyRandom::new(&[3u8]));

    // Initialize 1:
    let funder_incoming = FunderIncoming::Init;
    await!(Box::pin(apply_funder_incoming(funder_incoming, &mut state1, &mut ephemeral1, 
                                 &mut rng, &mut identity_client1))).unwrap();

    // Initialize 2:
    let funder_incoming = FunderIncoming::Init;
    await!(Box::pin(apply_funder_incoming(funder_incoming, &mut state2, &mut ephemeral2, 
                                 &mut rng, &mut identity_client2))).unwrap();

    // Node1: Add friend 2:
    let add_friend = AddFriend {
        friend_public_key: pk2.clone(),
        address: 22u32,
        name: String::from("pk2"),
        balance: 0i128,
    };
    let incoming_control_message = FunderIncomingControl::AddFriend(add_friend);
    let funder_incoming = FunderIncoming::Control(incoming_control_message);
    await!(apply_funder_incoming(funder_incoming, &mut state1, &mut ephemeral1, 
                                 &mut rng, &mut identity_client1)).unwrap();

    // Node1: Enable friend 2:
    let set_friend_status = SetFriendStatus {
        friend_public_key: pk2.clone(),
        status: FriendStatus::Enabled,
    };
    let incoming_control_message = FunderIncomingControl::SetFriendStatus(set_friend_status);
    let funder_incoming = FunderIncoming::Control(incoming_control_message);
    await!(Box::pin(apply_funder_incoming(funder_incoming, &mut state1, &mut ephemeral1, 
                                 &mut rng, &mut identity_client1))).unwrap();

    // Node2: Add friend 1:
    let add_friend = AddFriend {
        friend_public_key: pk1.clone(),
        address: 11u32,
        name: String::from("pk1"),
        balance: 0i128,
    };
    let incoming_control_message = FunderIncomingControl::AddFriend(add_friend);
    let funder_incoming = FunderIncoming::Control(incoming_control_message);
    await!(Box::pin(apply_funder_incoming(funder_incoming, &mut state2, &mut ephemeral2, 
                                 &mut rng, &mut identity_client2))).unwrap();


    // Node2: enable friend 1:
    let set_friend_status = SetFriendStatus {
        friend_public_key: pk1.clone(),
        status: FriendStatus::Enabled,
    };
    let incoming_control_message = FunderIncomingControl::SetFriendStatus(set_friend_status);
    let funder_incoming = FunderIncoming::Control(incoming_control_message);
    await!(Box::pin(apply_funder_incoming(funder_incoming, &mut state2, &mut ephemeral2, 
                                 &mut rng, &mut identity_client2))).unwrap();


    // Node1: Notify that Node2 is alive
    // We expect that Node1 will resend his outgoing message when he is notified that Node1 is online.
    let incoming_liveness_message = IncomingLivenessMessage::Online(pk2.clone());
    let funder_incoming = FunderIncoming::Comm(FunderIncomingComm::Liveness(incoming_liveness_message));
    let (outgoing_comms, _outgoing_control) = await!(Box::pin(apply_funder_incoming(funder_incoming, &mut state1, &mut ephemeral1, 
                                 &mut rng, &mut identity_client1))).unwrap();

    assert_eq!(outgoing_comms.len(), 1);
    let friend_message = match &outgoing_comms[0] {
        FunderOutgoingComm::FriendMessage((pk, friend_message)) => {
            if let FriendMessage::MoveTokenRequest(move_token_request) = friend_message {
                assert_eq!(pk, &pk2);
                assert_eq!(move_token_request.token_wanted, true);

                let friend_move_token = &move_token_request.friend_move_token;
                assert_eq!(friend_move_token.move_token_counter, 0);
                assert_eq!(friend_move_token.inconsistency_counter, 0);
                assert_eq!(friend_move_token.balance, 0);
                assert_eq!(friend_move_token.opt_local_address, None);

            } else {
                unreachable!();
            }
            friend_message.clone()
        },
        _ => unreachable!(),
    };

    // Node2: Notify that Node1 is alive
    let incoming_liveness_message = IncomingLivenessMessage::Online(pk1.clone());
    let funder_incoming = FunderIncoming::Comm(FunderIncomingComm::Liveness(incoming_liveness_message));
    await!(Box::pin(apply_funder_incoming(funder_incoming, &mut state2, &mut ephemeral2, 
                                 &mut rng, &mut identity_client2))).unwrap();

    // Node2: Receive friend_message from Node1:
    let funder_incoming = FunderIncoming::Comm(FunderIncomingComm::Friend((pk1.clone(), friend_message)));
    let (outgoing_comms, _outgoing_control) = await!(Box::pin(apply_funder_incoming(funder_incoming, &mut state2, &mut ephemeral2, 
                                 &mut rng, &mut identity_client2))).unwrap();


    assert_eq!(outgoing_comms.len(), 1);
    let friend_message = match &outgoing_comms[0] {
        FunderOutgoingComm::FriendMessage((pk, friend_message)) => {
            if let FriendMessage::MoveTokenRequest(move_token_request) = friend_message {
                assert_eq!(pk, &pk1);
                assert_eq!(move_token_request.token_wanted, true);
                let friend_move_token = &move_token_request.friend_move_token;
                assert!(friend_move_token.operations.is_empty());

                assert_eq!(friend_move_token.move_token_counter, 1);
                assert_eq!(friend_move_token.inconsistency_counter, 0);
                assert_eq!(friend_move_token.balance, 0);
                assert_eq!(friend_move_token.opt_local_address, Some(0x1338));
            } else {
                unreachable!();
            }
            friend_message.clone()
        },
        _ => unreachable!(),
    };


    // Node1: Receive friend_message from Node2:
    let funder_incoming = FunderIncoming::Comm(FunderIncomingComm::Friend((pk2.clone(), friend_message)));
    let (outgoing_comms, _outgoing_control) = await!(Box::pin(apply_funder_incoming(funder_incoming, &mut state1, &mut ephemeral1, 
                                 &mut rng, &mut identity_client1))).unwrap();

    assert_eq!(outgoing_comms.len(), 2);
    let friend_message = match &outgoing_comms[1] {
        FunderOutgoingComm::FriendMessage((pk, friend_message)) => {
            if let FriendMessage::MoveTokenRequest(move_token_request) = friend_message {
                assert_eq!(pk, &pk2);
                assert_eq!(move_token_request.token_wanted, true);
                let friend_move_token = &move_token_request.friend_move_token;
                assert!(friend_move_token.operations.is_empty());

                assert_eq!(friend_move_token.move_token_counter, 2);
                assert_eq!(friend_move_token.inconsistency_counter, 0);
                assert_eq!(friend_move_token.balance, 0);
                assert_eq!(friend_move_token.opt_local_address, Some(0x1337));
            } else {
                unreachable!();
            }
            friend_message.clone()
        },
        _ => unreachable!(),
    };

    // Node2: Receive friend_message from Node1:
    let funder_incoming = FunderIncoming::Comm(FunderIncomingComm::Friend((pk1.clone(), friend_message)));
    let (outgoing_comms, _outgoing_control) = await!(Box::pin(apply_funder_incoming(funder_incoming, &mut state2, &mut ephemeral2, 
                                 &mut rng, &mut identity_client2))).unwrap();

    assert_eq!(outgoing_comms.len(), 1);
    let friend_message = match &outgoing_comms[0] {
        FunderOutgoingComm::FriendMessage((pk, friend_message)) => {
            if let FriendMessage::MoveTokenRequest(move_token_request) = friend_message {
                assert_eq!(pk, &pk1);
                assert_eq!(move_token_request.token_wanted, false);
                let friend_move_token = &move_token_request.friend_move_token;
                assert!(friend_move_token.operations.is_empty());

                assert_eq!(friend_move_token.move_token_counter, 3);
                assert_eq!(friend_move_token.inconsistency_counter, 0);
                assert_eq!(friend_move_token.balance, 0);
                assert_eq!(friend_move_token.opt_local_address, None);
            } else {
                unreachable!();
            }
            friend_message.clone()
        },
        _ => unreachable!(),
    };

    // Node1: Receive friend_message from Node2:
    let funder_incoming = FunderIncoming::Comm(FunderIncomingComm::Friend((pk2.clone(), friend_message)));
    let (outgoing_comms, _outgoing_control) = await!(Box::pin(apply_funder_incoming(funder_incoming, &mut state1, &mut ephemeral1, 
                                 &mut rng, &mut identity_client1))).unwrap();
    assert!(outgoing_comms.is_empty());


    // Node1 decides to change his address:
    let incoming_control_message = FunderIncomingControl::SetAddress(Some(0x2337));
    let funder_incoming = FunderIncoming::Control(incoming_control_message);
    let (outgoing_comms, _outgoing_control) = await!(apply_funder_incoming(funder_incoming, &mut state1, &mut ephemeral1, 
                                 &mut rng, &mut identity_client1)).unwrap();

    // Node1 sends an update about his new address:
    assert_eq!(outgoing_comms.len(), 3);

    match &outgoing_comms[0] {
        FunderOutgoingComm::ChannelerConfig(ChannelerConfig::SetAddress(Some(0x2337))) => {},
        _ => unreachable!(),
    };

    match &outgoing_comms[1] {
        FunderOutgoingComm::ChannelerConfig(ChannelerConfig::UpdateFriend(update_friend)) => {
            assert_eq!(update_friend.friend_public_key, pk2);
            assert_eq!(update_friend.friend_address, 0x1338);
            assert_eq!(update_friend.local_addresses, vec![0x2337, 0x1337]);
        },
        _ => unreachable!(),
    };

    let friend_message = match &outgoing_comms[2] {
        FunderOutgoingComm::FriendMessage((pk, friend_message)) => {
            if let FriendMessage::MoveTokenRequest(move_token_request) = friend_message {
                assert_eq!(pk, &pk2);
                assert_eq!(move_token_request.token_wanted, true);
                let friend_move_token = &move_token_request.friend_move_token;
                assert!(friend_move_token.operations.is_empty());

                assert_eq!(friend_move_token.move_token_counter, 4);
                assert_eq!(friend_move_token.inconsistency_counter, 0);
                assert_eq!(friend_move_token.balance, 0);
                assert_eq!(friend_move_token.opt_local_address, Some(0x2337));
            } else {
                unreachable!();
            }
            friend_message.clone()
        },
        _ => unreachable!(),
    };

    // Node2: Receive friend_message from Node1:
    let funder_incoming = FunderIncoming::Comm(FunderIncomingComm::Friend((pk1.clone(), friend_message)));
    let (outgoing_comms, _outgoing_control) = await!(Box::pin(apply_funder_incoming(funder_incoming, &mut state2, &mut ephemeral2, 
                                 &mut rng, &mut identity_client2))).unwrap();

    assert_eq!(outgoing_comms.len(), 1);
    let friend_message = match &outgoing_comms[0] {
        FunderOutgoingComm::FriendMessage((pk, friend_message)) => {
            if let FriendMessage::MoveTokenRequest(move_token_request) = friend_message {
                assert_eq!(pk, &pk1);
                assert_eq!(move_token_request.token_wanted, false);
                let friend_move_token = &move_token_request.friend_move_token;
                assert!(friend_move_token.operations.is_empty());

                assert_eq!(friend_move_token.move_token_counter, 5);
                assert_eq!(friend_move_token.inconsistency_counter, 0);
                assert_eq!(friend_move_token.balance, 0);
                assert_eq!(friend_move_token.opt_local_address, None);
            } else {
                unreachable!();
            }
            friend_message.clone()
        },
        _ => unreachable!(),
    };

    // Node1: Receive friend_message from Node2:
    let funder_incoming = FunderIncoming::Comm(FunderIncomingComm::Friend((pk2.clone(), friend_message)));
    let (outgoing_comms, _outgoing_control) = await!(Box::pin(apply_funder_incoming(funder_incoming, &mut state1, &mut ephemeral1, 
                                 &mut rng, &mut identity_client1))).unwrap();
    assert_eq!(outgoing_comms.len(), 1);
    match &outgoing_comms[0] {
        FunderOutgoingComm::ChannelerConfig(ChannelerConfig::UpdateFriend(update_friend)) => {
            assert_eq!(update_friend.friend_public_key, pk2);
            assert_eq!(update_friend.friend_address, 0x1338);
            assert_eq!(update_friend.local_addresses, vec![0x2337]);
        },
        _ => unreachable!(),
    };
}

#[test]
fn test_handler_change_address() {
    let mut thread_pool = ThreadPool::new().unwrap();

    let rng1 = DummyRandom::new(&[1u8]);
    let pkcs8 = generate_pkcs8_key_pair(&rng1);
    let identity1 = SoftwareEd25519Identity::from_pkcs8(&pkcs8).unwrap();
    let (requests_sender1, identity_server1) = create_identity(identity1);
    let identity_client1 = IdentityClient::new(requests_sender1);
    thread_pool.spawn(identity_server1.then(|_| future::ready(()))).unwrap();

    let rng2 = DummyRandom::new(&[2u8]);
    let pkcs8 = generate_pkcs8_key_pair(&rng2);
    let identity2 = SoftwareEd25519Identity::from_pkcs8(&pkcs8).unwrap();
    let (requests_sender2, identity_server2) = create_identity(identity2);
    let identity_client2 = IdentityClient::new(requests_sender2);
    thread_pool.spawn(identity_server2.then(|_| future::ready(()))).unwrap();

    thread_pool.run(task_handler_change_address(identity_client1, identity_client2));
}
