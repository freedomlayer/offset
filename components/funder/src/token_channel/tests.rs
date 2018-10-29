use crypto::identity::{PublicKey, PUBLIC_KEY_LEN};
use crypto::uid::{Uid, UID_LEN};
#[allow(unused)]
use crypto::crypto_rand::{RandValue, RAND_VALUE_LEN};
use crate::token_channel::types::{/*TcRequestsStatus, */TokenChannel};
#[allow(unused)]
use crate::types::{RequestsStatus, InvoiceId, INVOICE_ID_LEN, 
    FunderFreezeLink, Ratio, FriendsRoute, 
    RequestSendFunds, ResponseSendFunds, FriendTcOp};

use crate::token_channel::outgoing::{OutgoingTc, QueueOperationFailure};
use crate::token_channel::incoming::{process_operation, ProcessOperationOutput, ProcessOperationError};

/// Helper function for applying an outgoing operation over a token channel.
fn apply_outgoing(token_channel: &mut TokenChannel, friend_tc_op: FriendTcOp) 
    -> Result<(), QueueOperationFailure> {

    let max_operations = 1;
    let mut outgoing = OutgoingTc::new(token_channel, max_operations);
    assert!(outgoing.is_operations_empty());
    outgoing.queue_operation(friend_tc_op)?;
    assert!(!outgoing.is_operations_empty());
    let (_operations, mutations) = outgoing.done();

    for mutation in mutations {
        token_channel.mutate(&mutation);
    }
    Ok(())
}

/// Helper function for applying an incoming operation over a token channel.
fn apply_incoming(mut token_channel: &mut TokenChannel, friend_tc_op: FriendTcOp) -> 
    Result<ProcessOperationOutput, ProcessOperationError> {
        
    process_operation(&mut token_channel,friend_tc_op)
}

#[test]
fn test_outgoing_open_close_requests() {
    let local_public_key = PublicKey::from(&[0xaa; PUBLIC_KEY_LEN]);
    let remote_public_key = PublicKey::from(&[0xbb; PUBLIC_KEY_LEN]);
    let balance = 0;
    let mut token_channel = TokenChannel::new(&local_public_key,
                                          &remote_public_key,
                                           balance);

    assert_eq!(token_channel.state().requests_status.local, RequestsStatus::Closed);
    assert_eq!(token_channel.state().requests_status.remote, RequestsStatus::Closed);

    apply_outgoing(&mut token_channel, FriendTcOp::EnableRequests).unwrap();
    assert_eq!(token_channel.state().requests_status.local, RequestsStatus::Open);
    assert_eq!(token_channel.state().requests_status.remote, RequestsStatus::Closed);

    apply_outgoing(&mut token_channel, FriendTcOp::DisableRequests).unwrap();
    assert_eq!(token_channel.state().requests_status.local, RequestsStatus::Closed);
    assert_eq!(token_channel.state().requests_status.remote, RequestsStatus::Closed);
}

#[test]
fn test_outgoing_set_remote_max_debt() {
    let local_public_key = PublicKey::from(&[0xaa; PUBLIC_KEY_LEN]);
    let remote_public_key = PublicKey::from(&[0xbb; PUBLIC_KEY_LEN]);
    let balance = 0;
    let mut token_channel = TokenChannel::new(&local_public_key,
                                          &remote_public_key,
                                           balance);

    assert_eq!(token_channel.state().balance.remote_max_debt, 0);
    apply_outgoing(&mut token_channel, FriendTcOp::SetRemoteMaxDebt(20)).unwrap();
    assert_eq!(token_channel.state().balance.remote_max_debt, 20);
}

#[test]
fn test_request_response_send_funds() {
    let local_public_key = PublicKey::from(&[0xaa; PUBLIC_KEY_LEN]);
    let remote_public_key = PublicKey::from(&[0xbb; PUBLIC_KEY_LEN]);
    let balance = 0;
    let mut token_channel = TokenChannel::new(&local_public_key,
                                          &remote_public_key,
                                           balance);


    // Make enough trust from remote side, so that we will be able to send credits:
    apply_incoming(&mut token_channel, FriendTcOp::SetRemoteMaxDebt(100)).unwrap();

    let request_id = Uid::from(&[3; UID_LEN]);
    let route = FriendsRoute {
        public_keys: vec![PublicKey::from(&[0xaa; PUBLIC_KEY_LEN]),
                          PublicKey::from(&[0xbb; PUBLIC_KEY_LEN]),
                          PublicKey::from(&[0xcc; PUBLIC_KEY_LEN])],
    };
    let invoice_id = InvoiceId::from(&[0; INVOICE_ID_LEN]);

    let funder_freeze_link = FunderFreezeLink {
        shared_credits: 80,
        usable_ratio: Ratio::One,
    };

    let request_send_funds = RequestSendFunds {
        request_id: request_id.clone(),
        route,
        dest_payment: 10,
        invoice_id,
        freeze_links: vec![funder_freeze_link],
    };

    apply_outgoing(&mut token_channel, FriendTcOp::RequestSendFunds(request_send_funds)).unwrap();

    assert_eq!(token_channel.state().balance.balance, 0);
    assert_eq!(token_channel.state().balance.local_max_debt, 100);
    assert_eq!(token_channel.state().balance.remote_max_debt, 0);
    assert_eq!(token_channel.state().balance.local_pending_debt, 11);
    assert_eq!(token_channel.state().balance.remote_pending_debt, 0);

    /*
    let rand_nonce = RandValue::from(&[5; RAND_VALUE_LEN]);

    // TODO: Add signature here:
    let response_send_funds = ResponseSendFunds {
        request_id: request_id.clone(),
        rand_nonce: rand_nonce.clone(),
        signature: Signature,
    };

    apply_incoming(&mut tokne_channel, FriendTcOp::ResponseSendFunds(response_send_funds)).unwrap();

    assert_eq!(token_channel.state().balance.balance, -11);
    assert_eq!(token_channel.state().balance.local_max_debt, 100);
    assert_eq!(token_channel.state().balance.remote_max_debt, 0);
    assert_eq!(token_channel.state().balance.local_pending_debt, 0);
    assert_eq!(token_channel.state().balance.remote_pending_debt, 0);
    */
}
