use crypto::identity::{PublicKey, PUBLIC_KEY_LEN};
use crypto::test_utils::DummyRandom;
use crypto::uid::{Uid, UID_LEN};
use crypto::identity::{Identity, SoftwareEd25519Identity,
                        generate_pkcs8_key_pair, SIGNATURE_LEN, Signature};

use crypto::crypto_rand::{RandValue, RAND_VALUE_LEN};

use proto::funder::messages::{InvoiceId, INVOICE_ID_LEN, FreezeLink, Ratio, FriendsRoute, 
    RequestSendFunds, ResponseSendFunds, FailureSendFunds, FriendTcOp};

use crate::mutual_credit::types::MutualCredit;
#[allow(unused)]
use crate::types::{RequestsStatus, create_pending_request};
use crate::signature_buff::{create_response_signature_buffer, 
    create_failure_signature_buffer};

use crate::mutual_credit::outgoing::{OutgoingMc, QueueOperationFailure};
use crate::mutual_credit::incoming::{process_operation, ProcessOperationOutput, ProcessOperationError};

/// Helper function for applying an outgoing operation over a token channel.
fn apply_outgoing(mutual_credit: &mut MutualCredit, friend_tc_op: FriendTcOp) 
    -> Result<(), QueueOperationFailure> {

    let max_operations = 1;
    let mut outgoing = OutgoingMc::new(mutual_credit, max_operations);
    assert!(outgoing.is_operations_empty());
    outgoing.queue_operation(friend_tc_op)?;
    assert!(!outgoing.is_operations_empty());
    let (_operations, mutations) = outgoing.done();

    for mutation in mutations {
        mutual_credit.mutate(&mutation);
    }
    Ok(())
}

/// Helper function for applying an incoming operation over a token channel.
fn apply_incoming(mut mutual_credit: &mut MutualCredit, friend_tc_op: FriendTcOp) -> 
    Result<ProcessOperationOutput, ProcessOperationError> {
        
    process_operation(&mut mutual_credit,friend_tc_op)
}

#[test]
fn test_outgoing_open_close_requests() {
    let local_public_key = PublicKey::from(&[0xaa; PUBLIC_KEY_LEN]);
    let remote_public_key = PublicKey::from(&[0xbb; PUBLIC_KEY_LEN]);
    let balance = 0;
    let mut mutual_credit = MutualCredit::new(&local_public_key,
                                          &remote_public_key,
                                           balance);

    assert_eq!(mutual_credit.state().requests_status.local, RequestsStatus::Closed);
    assert_eq!(mutual_credit.state().requests_status.remote, RequestsStatus::Closed);

    apply_outgoing(&mut mutual_credit, FriendTcOp::EnableRequests).unwrap();
    assert_eq!(mutual_credit.state().requests_status.local, RequestsStatus::Open);
    assert_eq!(mutual_credit.state().requests_status.remote, RequestsStatus::Closed);

    apply_outgoing(&mut mutual_credit, FriendTcOp::DisableRequests).unwrap();
    assert_eq!(mutual_credit.state().requests_status.local, RequestsStatus::Closed);
    assert_eq!(mutual_credit.state().requests_status.remote, RequestsStatus::Closed);
}

#[test]
fn test_outgoing_set_remote_max_debt() {
    let local_public_key = PublicKey::from(&[0xaa; PUBLIC_KEY_LEN]);
    let remote_public_key = PublicKey::from(&[0xbb; PUBLIC_KEY_LEN]);
    let balance = 0;
    let mut mutual_credit = MutualCredit::new(&local_public_key,
                                          &remote_public_key,
                                           balance);

    assert_eq!(mutual_credit.state().balance.remote_max_debt, 0);
    apply_outgoing(&mut mutual_credit, FriendTcOp::SetRemoteMaxDebt(20)).unwrap();
    assert_eq!(mutual_credit.state().balance.remote_max_debt, 20);
}

#[test]
fn test_request_response_send_funds() {
    let local_public_key = PublicKey::from(&[0xaa; PUBLIC_KEY_LEN]);
    let remote_public_key = PublicKey::from(&[0xbb; PUBLIC_KEY_LEN]);
    let balance = 0;
    let mut mutual_credit = MutualCredit::new(&local_public_key,
                                          &remote_public_key,
                                           balance);


    // Make enough trust from remote side, so that we will be able to send credits:
    apply_incoming(&mut mutual_credit, FriendTcOp::SetRemoteMaxDebt(100)).unwrap();

    // Remote side should open his requests status:
    apply_incoming(&mut mutual_credit, FriendTcOp::EnableRequests).unwrap();

    let rng = DummyRandom::new(&[1u8]);
    let pkcs8 = generate_pkcs8_key_pair(&rng);
    let identity = SoftwareEd25519Identity::from_pkcs8(&pkcs8).unwrap();
    let public_key_c = identity.get_public_key();

    let request_id = Uid::from(&[3; UID_LEN]);
    let route = FriendsRoute {
        public_keys: vec![PublicKey::from(&[0xaa; PUBLIC_KEY_LEN]),
                          PublicKey::from(&[0xbb; PUBLIC_KEY_LEN]),
                          public_key_c.clone()],
    };
    let invoice_id = InvoiceId::from(&[0; INVOICE_ID_LEN]);

    let funder_freeze_link = FreezeLink {
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

    let pending_request = create_pending_request(&request_send_funds);
    apply_outgoing(&mut mutual_credit, FriendTcOp::RequestSendFunds(request_send_funds)).unwrap();

    assert_eq!(mutual_credit.state().balance.balance, 0);
    assert_eq!(mutual_credit.state().balance.local_max_debt, 100);
    assert_eq!(mutual_credit.state().balance.remote_max_debt, 0);
    let local_pending_debt = mutual_credit.state().balance.local_pending_debt;
    assert!(local_pending_debt > 0);
    assert_eq!(mutual_credit.state().balance.remote_pending_debt, 0);


    let rand_nonce = RandValue::from(&[5; RAND_VALUE_LEN]);

    // TODO: Add signature here:
    let mut response_send_funds = ResponseSendFunds {
        request_id: request_id.clone(),
        rand_nonce: rand_nonce.clone(),
        signature: Signature::from(&[0; SIGNATURE_LEN]),
    };

    let sign_buffer = create_response_signature_buffer(&response_send_funds, 
                                     &pending_request);
    response_send_funds.signature = identity.sign(&sign_buffer);

    apply_incoming(&mut mutual_credit, FriendTcOp::ResponseSendFunds(response_send_funds)).unwrap();

    let balance = mutual_credit.state().balance.balance;
    assert_eq!(balance, -(local_pending_debt as i128));
    assert_eq!(mutual_credit.state().balance.local_max_debt, 100);
    assert_eq!(mutual_credit.state().balance.remote_max_debt, 0);
    assert_eq!(mutual_credit.state().balance.local_pending_debt, 0);
    assert_eq!(mutual_credit.state().balance.remote_pending_debt, 0);
}

#[test]
fn test_request_failure_send_funds() {
    let rng = DummyRandom::new(&[1u8]);
    let pkcs8 = generate_pkcs8_key_pair(&rng);
    let identity = SoftwareEd25519Identity::from_pkcs8(&pkcs8).unwrap();
    let public_key_b = identity.get_public_key();

    let local_public_key = PublicKey::from(&[0xaa; PUBLIC_KEY_LEN]);
    let remote_public_key = public_key_b.clone();
    let balance = 0;
    let mut mutual_credit = MutualCredit::new(&local_public_key,
                                          &remote_public_key,
                                           balance);


    // Make enough trust from remote side, so that we will be able to send credits:
    apply_incoming(&mut mutual_credit, FriendTcOp::SetRemoteMaxDebt(100)).unwrap();

    // Remote side should open his requests status:
    apply_incoming(&mut mutual_credit, FriendTcOp::EnableRequests).unwrap();


    let request_id = Uid::from(&[3; UID_LEN]);
    let route = FriendsRoute {
        public_keys: vec![PublicKey::from(&[0xaa; PUBLIC_KEY_LEN]),
                          public_key_b.clone(),
                          PublicKey::from(&[0xcc; PUBLIC_KEY_LEN])],
    };
    let invoice_id = InvoiceId::from(&[0; INVOICE_ID_LEN]);

    let funder_freeze_link = FreezeLink {
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

    let pending_request = create_pending_request(&request_send_funds);
    apply_outgoing(&mut mutual_credit, FriendTcOp::RequestSendFunds(request_send_funds)).unwrap();

    assert_eq!(mutual_credit.state().balance.balance, 0);
    assert_eq!(mutual_credit.state().balance.local_max_debt, 100);
    assert_eq!(mutual_credit.state().balance.remote_max_debt, 0);
    let local_pending_debt = mutual_credit.state().balance.local_pending_debt;
    assert!(local_pending_debt > 0);
    assert_eq!(mutual_credit.state().balance.remote_pending_debt, 0);


    let rand_nonce = RandValue::from(&[5; RAND_VALUE_LEN]);

    let mut failure_send_funds = FailureSendFunds {
        request_id,
        reporting_public_key: public_key_b.clone(),
        rand_nonce,
        signature: Signature::from(&[0; SIGNATURE_LEN]),
    };

    let sign_buffer = create_failure_signature_buffer(&failure_send_funds, 
                                     &pending_request);
    failure_send_funds.signature = identity.sign(&sign_buffer);

    apply_incoming(&mut mutual_credit, FriendTcOp::FailureSendFunds(failure_send_funds)).unwrap();

    assert_eq!(mutual_credit.state().balance.balance, 0);
    assert_eq!(mutual_credit.state().balance.local_max_debt, 100);
    assert_eq!(mutual_credit.state().balance.remote_max_debt, 0);
    assert_eq!(mutual_credit.state().balance.local_pending_debt, 0);
    assert_eq!(mutual_credit.state().balance.remote_pending_debt, 0);
}
