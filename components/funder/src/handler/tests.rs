use super::*;

use futures::executor::ThreadPool;
use futures::{future, FutureExt};
use futures::task::SpawnExt;

use identity::{create_identity, IdentityClient};

use crypto::test_utils::DummyRandom;
use crypto::identity::{SoftwareEd25519Identity, generate_pkcs8_key_pair};
use crypto::crypto_rand::{RngContainer, CryptoRandom};
use crypto::uid::{Uid, UID_LEN};

use crate::token_channel::{is_public_key_lower};

use crate::types::{FunderIncoming, IncomingControlMessage, 
    AddFriend, IncomingLivenessMessage, FriendStatus,
    SetFriendStatus, FriendMessage, SetFriendRemoteMaxDebt,
    FriendsRoute, UserRequestSendFunds, InvoiceId, INVOICE_ID_LEN, 
    SetRequestsStatus, RequestsStatus};


/// A helper function. Applies an incoming funder message, updating state and ephemeral
/// accordingly:
async fn apply_funder_incoming<'a,A: Clone + 'static,R: CryptoRandom + 'static>(funder_incoming: FunderIncoming<A>,
                               state: &'a mut FunderState<A>, 
                               ephemeral: &'a mut Ephemeral, 
                               rng: R, 
                               identity_client: IdentityClient) 
                -> Result<(Vec<FunderOutgoingComm<A>>, Vec<FunderOutgoingControl<A>>), FunderHandlerError> {

    let funder_handler_output = await!(funder_handle_message(identity_client,
                          rng,
                          state.clone(),
                          ephemeral.clone(),
                          funder_incoming))?;

    let FunderHandlerOutput {ephemeral_mutations, funder_mutations, outgoing_comms, outgoing_control}
        = funder_handler_output;

    // Mutate FunderState according to the mutations:
    for mutation in &funder_mutations {
        state.mutate(mutation);
    }

    // Mutate Ephemeral according to the mutations:
    for mutation in &ephemeral_mutations {
        ephemeral.mutate(mutation);
    }

    Ok((outgoing_comms, outgoing_control))
}

async fn task_handler_pair_basic(identity_client1: IdentityClient, 
                                 identity_client2: IdentityClient) {
    // NOTE: We use Box::pinned() in order to make sure we don't get a too large Future which will
    // cause a stack overflow. 
    // See:  https://github.com/rust-lang-nursery/futures-rs/issues/1330 
    
    // Sort the identities. identity_client1 will be the first sender:
    let pk1 = await!(identity_client1.request_public_key()).unwrap();
    let pk2 = await!(identity_client2.request_public_key()).unwrap();
    let (identity_client1, pk1, identity_client2, pk2) = if is_public_key_lower(&pk1, &pk2) {
        (identity_client1, pk1, identity_client2, pk2)
    } else {
        (identity_client2, pk2, identity_client1, pk1)
    };

    let mut state1 = FunderState::<u32>::new(&pk1);
    let mut ephemeral1 = Ephemeral::new(&state1);
    let mut state2 = FunderState::<u32>::new(&pk2);
    let mut ephemeral2 = Ephemeral::new(&state2);

    let rng = RngContainer::new(DummyRandom::new(&[3u8]));

    // Initialize 1:
    let funder_incoming = FunderIncoming::Init;
    await!(Box::pinned(apply_funder_incoming(funder_incoming, &mut state1, &mut ephemeral1, 
                                 rng.clone(), identity_client1.clone()))).unwrap();

    // Initialize 2:
    let funder_incoming = FunderIncoming::Init;
    await!(Box::pinned(apply_funder_incoming(funder_incoming, &mut state2, &mut ephemeral2, 
                                 rng.clone(), identity_client2.clone()))).unwrap();

    // Node1: Add friend 2:
    let add_friend = AddFriend {
        friend_public_key: pk2.clone(),
        address: 22u32,
        name: String::from("pk2"),
        balance: 0i128,
    };
    let incoming_control_message = IncomingControlMessage::AddFriend(add_friend);
    let funder_incoming = FunderIncoming::Control(incoming_control_message);
    await!(apply_funder_incoming(funder_incoming, &mut state1, &mut ephemeral1, 
                                 rng.clone(), identity_client1.clone())).unwrap();

    // Node1: Enable friend 2:
    let set_friend_status = SetFriendStatus {
        friend_public_key: pk2.clone(),
        status: FriendStatus::Enable,
    };
    let incoming_control_message = IncomingControlMessage::SetFriendStatus(set_friend_status);
    let funder_incoming = FunderIncoming::Control(incoming_control_message);
    await!(Box::pinned(apply_funder_incoming(funder_incoming, &mut state1, &mut ephemeral1, 
                                 rng.clone(), identity_client1.clone()))).unwrap();

    // Node2: Add friend 1:
    let add_friend = AddFriend {
        friend_public_key: pk1.clone(),
        address: 11u32,
        name: String::from("pk1"),
        balance: 0i128,
    };
    let incoming_control_message = IncomingControlMessage::AddFriend(add_friend);
    let funder_incoming = FunderIncoming::Control(incoming_control_message);
    await!(Box::pinned(apply_funder_incoming(funder_incoming, &mut state2, &mut ephemeral2, 
                                 rng.clone(), identity_client2.clone()))).unwrap();


    // Node2: enable friend 1:
    let set_friend_status = SetFriendStatus {
        friend_public_key: pk1.clone(),
        status: FriendStatus::Enable,
    };
    let incoming_control_message = IncomingControlMessage::SetFriendStatus(set_friend_status);
    let funder_incoming = FunderIncoming::Control(incoming_control_message);
    await!(Box::pinned(apply_funder_incoming(funder_incoming, &mut state2, &mut ephemeral2, 
                                 rng.clone(), identity_client2.clone()))).unwrap();


    // Node1: Notify that Node2 is alive
    // We expect that Node1 will resend his outgoing message when he is notified that Node1 is online.
    let incoming_liveness_message = IncomingLivenessMessage::Online(pk2.clone());
    let funder_incoming = FunderIncoming::Comm(IncomingCommMessage::Liveness(incoming_liveness_message));
    let (outgoing_comms, _outgoing_control) = await!(Box::pinned(apply_funder_incoming(funder_incoming, &mut state1, &mut ephemeral1, 
                                 rng.clone(), identity_client1.clone()))).unwrap();

    assert_eq!(outgoing_comms.len(), 1);
    let friend_message = match &outgoing_comms[0] {
        FunderOutgoingComm::FriendMessage((pk, friend_message)) => {
            if let FriendMessage::MoveTokenRequest(move_token_request) = friend_message {
                assert_eq!(pk, &pk2);
                assert_eq!(move_token_request.token_wanted, false);

                let friend_move_token = &move_token_request.friend_move_token;
                assert_eq!(friend_move_token.move_token_counter, 0);
                assert_eq!(friend_move_token.inconsistency_counter, 0);
                assert_eq!(friend_move_token.balance, 0);

            } else {
                unreachable!();
            }
            friend_message.clone()
        },
        _ => unreachable!(),
    };

    // Node2: Notify that Node1 is alive
    let incoming_liveness_message = IncomingLivenessMessage::Online(pk1.clone());
    let funder_incoming = FunderIncoming::Comm(IncomingCommMessage::Liveness(incoming_liveness_message));
    await!(Box::pinned(apply_funder_incoming(funder_incoming, &mut state2, &mut ephemeral2, 
                                 rng.clone(), identity_client2.clone()))).unwrap();

    // Node2: Receive friend_message from Node1:
    let funder_incoming = FunderIncoming::Comm(IncomingCommMessage::Friend((pk1.clone(), friend_message)));
    await!(Box::pinned(apply_funder_incoming(funder_incoming, &mut state2, &mut ephemeral2, 
                                 rng.clone(), identity_client2.clone()))).unwrap();


    // Node1 receives control message to set remote max debt:
    let set_friend_remote_max_debt = SetFriendRemoteMaxDebt {
        friend_public_key: pk2.clone(),
        remote_max_debt: 100,
    };
    let incoming_control_message = IncomingControlMessage::SetFriendRemoteMaxDebt(set_friend_remote_max_debt);
    let funder_incoming = FunderIncoming::Control(incoming_control_message);
    let (outgoing_comms, _outgoing_control) = await!(Box::pinned(apply_funder_incoming(funder_incoming, &mut state1, &mut ephemeral1, 
                                 rng.clone(), identity_client1.clone()))).unwrap();
    // Node1 wants to obtain the token, so it resends the last outgoing move token with
    // token_wanted = true:
    assert_eq!(outgoing_comms.len(), 1);
    let friend_message = match &outgoing_comms[0] {
        FunderOutgoingComm::FriendMessage((pk, friend_message)) => {
            if let FriendMessage::MoveTokenRequest(move_token_request) = friend_message {
                assert_eq!(pk, &pk2);
                assert_eq!(move_token_request.token_wanted, true);
            } else {
                unreachable!();
            }
            friend_message.clone()
        },
        _ => unreachable!(),
    };

    // Node2: Receive friend_message (request for token) from Node1:
    let funder_incoming = FunderIncoming::Comm(IncomingCommMessage::Friend((pk1.clone(), friend_message)));
    let (outgoing_comms, _outgoing_control) = await!(Box::pinned(apply_funder_incoming(funder_incoming, &mut state2, &mut ephemeral2, 
                                 rng.clone(), identity_client2.clone()))).unwrap();
    assert_eq!(outgoing_comms.len(), 1);
    let friend_message = match &outgoing_comms[0] {
        FunderOutgoingComm::FriendMessage((pk, friend_message)) => {
            if let FriendMessage::MoveTokenRequest(_move_token_request) = friend_message {
                assert_eq!(pk, &pk1);
            } else {
                unreachable!();
            }
            friend_message.clone()
        },
        _ => unreachable!(),
    };

    // Node1: Receive friend_message from Node2:
    let funder_incoming = FunderIncoming::Comm(IncomingCommMessage::Friend((pk2.clone(), friend_message)));
    let (outgoing_comms, _outgoing_control) = await!(Box::pinned(apply_funder_incoming(funder_incoming, &mut state1, &mut ephemeral1, 
                                 rng.clone(), identity_client1.clone()))).unwrap();

    // Now that Node1 has the token, it will now send the SetRemoteMaxDebt message to Node2:
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
        },
        _ => unreachable!(),
    };

    // Node2: Receive friend_message (With SetRemoteMaxDebt) from Node1:
    let funder_incoming = FunderIncoming::Comm(IncomingCommMessage::Friend((pk1.clone(), friend_message)));
    let (_outgoing_comms, _outgoing_control) = await!(Box::pinned(apply_funder_incoming(funder_incoming, &mut state2, &mut ephemeral2, 
                                 rng.clone(), identity_client2.clone()))).unwrap();

    let friend2 = state1.friends.get(&pk2).unwrap();
    let remote_max_debt = match &friend2.channel_status {
        ChannelStatus::Consistent(token_channel) 
            => token_channel.get_mutual_credit().state().balance.remote_max_debt,
        _ => unreachable!(),
    };
    assert_eq!(remote_max_debt, 100);

    let friend1 = state2.friends.get(&pk1).unwrap();
    let local_max_debt = match &friend1.channel_status {
        ChannelStatus::Consistent(token_channel) 
            => token_channel.get_mutual_credit().state().balance.local_max_debt,
        _ => unreachable!(),
    };
    assert_eq!(local_max_debt, 100);

    // Node2 receives control message to send funds to Node1:
    // But Node1's requests are not open, therefore Node2 will return a ResponseReceived with
    // failure through the outgoing control:
    let user_request_send_funds = UserRequestSendFunds {
        request_id: Uid::from(&[3; UID_LEN]),
        route: FriendsRoute { public_keys: vec![pk2.clone(), pk1.clone()] },
        invoice_id: InvoiceId::from(&[1; INVOICE_ID_LEN]),
        dest_payment: 20,
    };
    let incoming_control_message = IncomingControlMessage::RequestSendFunds(user_request_send_funds);
    let funder_incoming = FunderIncoming::Control(incoming_control_message);
    let (outgoing_comms, outgoing_control) = await!(Box::pinned(apply_funder_incoming(funder_incoming, &mut state2, &mut ephemeral2, 
                                 rng.clone(), identity_client2.clone()))).unwrap();

    // Node2 will not send the RequestFunds message to Node1, because he knows Node1 is
    // not ready:
    assert_eq!(outgoing_comms.len(), 0);
    assert_eq!(outgoing_control.len(), 1);

    // Checking the current requests status on the mutual credit:
    let friend2 = state1.friends.get(&pk2).unwrap();
    let mutual_credit_state = match &friend2.channel_status {
        ChannelStatus::Consistent(token_channel) 
            => token_channel.get_mutual_credit().state(),
        _ => unreachable!(),
    };
    assert_eq!(mutual_credit_state.requests_status.local, RequestsStatus::Closed);
    assert_eq!(mutual_credit_state.requests_status.remote, RequestsStatus::Closed);

    // Node1 gets a control message to declare his requests are open,
    // However, Node1 doesn't have the token at this moment.
    let set_requests_status = SetRequestsStatus {
        friend_public_key: pk2.clone(),
        status: RequestsStatus::Open,
    };
    let incoming_control_message = IncomingControlMessage::SetRequestsStatus(set_requests_status);
    let funder_incoming = FunderIncoming::Control(incoming_control_message);
    let (outgoing_comms, _outgoing_control) = await!(Box::pinned(apply_funder_incoming(funder_incoming, &mut state1, &mut ephemeral1, 
                                 rng.clone(), identity_client1.clone()))).unwrap();

    // Node1 will request the token:
    assert_eq!(outgoing_comms.len(), 1);
    let friend_message = if let FunderOutgoingComm::FriendMessage((_pk, friend_message)) = &outgoing_comms[0] {
        friend_message.clone()
    } else { unreachable!(); };

    // Node2 receives the request_token message:
    let funder_incoming = FunderIncoming::Comm(IncomingCommMessage::Friend((pk1.clone(), friend_message)));
    let (outgoing_comms, _outgoing_control) = await!(Box::pinned(apply_funder_incoming(funder_incoming, &mut state2, &mut ephemeral2, 
                                 rng.clone(), identity_client2.clone()))).unwrap();

    assert_eq!(outgoing_comms.len(), 1);
    let friend_message = if let FunderOutgoingComm::FriendMessage((_pk, friend_message)) = &outgoing_comms[0] {
        friend_message.clone()
    } else { unreachable!(); };

    // Node1 receives the token from Node2:
    let funder_incoming = FunderIncoming::Comm(IncomingCommMessage::Friend((pk2.clone(), friend_message)));
    let (outgoing_comms, _outgoing_control) = await!(Box::pinned(apply_funder_incoming(funder_incoming, &mut state1, &mut ephemeral1, 
                                 rng.clone(), identity_client1.clone()))).unwrap();

    // Node1 declares that his requests are open:
    let friend_message = if let FunderOutgoingComm::FriendMessage((_pk, friend_message)) = &outgoing_comms[0] {
        friend_message.clone()
    } else { unreachable!(); };

    // Node2 receives the set requests open message:
    let funder_incoming = FunderIncoming::Comm(IncomingCommMessage::Friend((pk1.clone(), friend_message)));
    let (_outgoing_comms, _outgoing_control) = await!(Box::pinned(apply_funder_incoming(funder_incoming, &mut state2, &mut ephemeral2, 
                                 rng.clone(), identity_client2.clone()))).unwrap();


    // Checking the current requests status on the mutual credit for Node1:
    let friend2 = state1.friends.get(&pk2).unwrap();
    let mutual_credit_state = match &friend2.channel_status {
        ChannelStatus::Consistent(token_channel) 
            => token_channel.get_mutual_credit().state(),
        _ => unreachable!(),
    };
    assert!(mutual_credit_state.requests_status.local.is_open());
    assert!(!mutual_credit_state.requests_status.remote.is_open());

    // Checking the current requests status on the mutual credit for Node2:
    let friend1 = state2.friends.get(&pk1).unwrap();
    let mutual_credit_state = match &friend1.channel_status {
        ChannelStatus::Consistent(token_channel) 
            => token_channel.get_mutual_credit().state(),
        _ => unreachable!(),
    };
    assert!(!mutual_credit_state.requests_status.local.is_open());
    assert!(mutual_credit_state.requests_status.remote.is_open());

    // Node2 receives control message to send funds to Node1:
    let user_request_send_funds = UserRequestSendFunds {
        request_id: Uid::from(&[3; UID_LEN]),
        route: FriendsRoute { public_keys: vec![pk2.clone(), pk1.clone()] },
        invoice_id: InvoiceId::from(&[1; INVOICE_ID_LEN]),
        dest_payment: 20,
    };
    let incoming_control_message = IncomingControlMessage::RequestSendFunds(user_request_send_funds);
    let funder_incoming = FunderIncoming::Control(incoming_control_message);
    let (outgoing_comms, _outgoing_control) = await!(Box::pinned(apply_funder_incoming(funder_incoming, &mut state2, &mut ephemeral2, 
                                 rng.clone(), identity_client2.clone()))).unwrap();


    // Node2 will send a RequestFunds message to Node1
    assert_eq!(outgoing_comms.len(), 1);
    let friend_message = if let FunderOutgoingComm::FriendMessage((_pk, friend_message)) = &outgoing_comms[0] {
        friend_message.clone()
    } else { unreachable!(); };

    // Node1 receives RequestSendFunds from Node2:
    let funder_incoming = FunderIncoming::Comm(IncomingCommMessage::Friend((pk2.clone(), friend_message)));
    let (outgoing_comms, _outgoing_control) = await!(Box::pinned(apply_funder_incoming(funder_incoming, &mut state1, &mut ephemeral1, 
                                 rng.clone(), identity_client1.clone()))).unwrap();

    // Node1 sends a ResponseSendFunds to Node2:
    assert_eq!(outgoing_comms.len(), 1);
    let friend_message = match &outgoing_comms[0] {
        FunderOutgoingComm::FriendMessage((pk, friend_message)) => {
            if let FriendMessage::MoveTokenRequest(move_token_request) = friend_message {
                assert_eq!(pk, &pk2);
                let friend_move_token = &move_token_request.friend_move_token;
                assert_eq!(friend_move_token.balance, 20);
                assert_eq!(friend_move_token.local_pending_debt, 0);
                assert_eq!(friend_move_token.remote_pending_debt, 0);
            } else {
                unreachable!();
            }
            friend_message.clone()
        },
        _ => unreachable!(),
    };

    // Node2 receives ResponseSendFunds from Node1:
    let funder_incoming = FunderIncoming::Comm(IncomingCommMessage::Friend((pk1.clone(), friend_message)));
    let (_outgoing_comms, _outgoing_control) = await!(Box::pinned(apply_funder_incoming(funder_incoming, &mut state2, &mut ephemeral2, 
                                 rng.clone(), identity_client2.clone()))).unwrap();

    // Current balance from Node1 point of view:
    let friend2 = state1.friends.get(&pk2).unwrap();
    let mutual_credit_state = match &friend2.channel_status {
        ChannelStatus::Consistent(token_channel) 
            => token_channel.get_mutual_credit().state(),
        _ => unreachable!(),
    };
    assert_eq!(mutual_credit_state.balance.balance, 20);
    assert_eq!(mutual_credit_state.balance.remote_pending_debt, 0);
    assert_eq!(mutual_credit_state.balance.local_pending_debt, 0);

    // Current balance from Node2 point of view:
    let friend1 = state2.friends.get(&pk1).unwrap();
    let mutual_credit_state = match &friend1.channel_status {
        ChannelStatus::Consistent(token_channel) 
            => token_channel.get_mutual_credit().state(),
        _ => unreachable!(),
    };
    assert_eq!(mutual_credit_state.balance.balance, -20);
    assert_eq!(mutual_credit_state.balance.remote_pending_debt, 0);
    assert_eq!(mutual_credit_state.balance.local_pending_debt, 0);

}

#[test]
fn test_handler_pair_basic() {
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

    thread_pool.run(task_handler_pair_basic(identity_client1, identity_client2));
}

