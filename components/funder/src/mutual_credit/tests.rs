use std::convert::TryFrom;

use crypto::hash_lock::HashLock;
use crypto::identity::{Identity, SoftwareEd25519Identity};
use crypto::rand::RandGen;
use crypto::test_utils::DummyRandom;

use proto::crypto::{InvoiceId, PlainLock, PrivateKey, PublicKey, RandValue, Signature, Uid};
use proto::funder::messages::{
    CancelSendFundsOp, CollectSendFundsOp, Currency, FriendTcOp, FriendsRoute, RequestSendFundsOp,
    RequestsStatus, ResponseSendFundsOp,
};
use signature::signature_buff::create_response_signature_buffer;

use crate::types::create_pending_transaction;

use crate::mutual_credit::incoming::{
    process_operation, ProcessOperationError, ProcessOperationOutput,
};
use crate::mutual_credit::outgoing::{OutgoingMc, QueueOperationError};
use crate::mutual_credit::types::MutualCredit;

/// Helper function for applying an outgoing operation over a token channel.
fn apply_outgoing(
    mutual_credit: &mut MutualCredit,
    friend_tc_op: &FriendTcOp,
) -> Result<(), QueueOperationError> {
    let mut outgoing = OutgoingMc::new(mutual_credit);
    let mutations = outgoing.queue_operation(friend_tc_op)?;

    for mutation in mutations {
        mutual_credit.mutate(&mutation);
    }
    Ok(())
}

/// Helper function for applying an incoming operation over a token channel.
fn apply_incoming(
    mut mutual_credit: &mut MutualCredit,
    friend_tc_op: FriendTcOp,
) -> Result<ProcessOperationOutput, ProcessOperationError> {
    process_operation(&mut mutual_credit, friend_tc_op)
}

#[test]
fn test_outgoing_open_close_requests() {
    let currency = Currency::try_from("OFFST".to_owned()).unwrap();

    let local_public_key = PublicKey::from(&[0xaa; PublicKey::len()]);
    let remote_public_key = PublicKey::from(&[0xbb; PublicKey::len()]);
    let balance = 0;
    let mut mutual_credit =
        MutualCredit::new(&local_public_key, &remote_public_key, &currency, balance);

    assert_eq!(
        mutual_credit.state().requests_status.local,
        RequestsStatus::Closed
    );
    assert_eq!(
        mutual_credit.state().requests_status.remote,
        RequestsStatus::Closed
    );

    apply_outgoing(&mut mutual_credit, &FriendTcOp::EnableRequests).unwrap();
    assert_eq!(
        mutual_credit.state().requests_status.local,
        RequestsStatus::Open
    );
    assert_eq!(
        mutual_credit.state().requests_status.remote,
        RequestsStatus::Closed
    );

    apply_outgoing(&mut mutual_credit, &FriendTcOp::DisableRequests).unwrap();
    assert_eq!(
        mutual_credit.state().requests_status.local,
        RequestsStatus::Closed
    );
    assert_eq!(
        mutual_credit.state().requests_status.remote,
        RequestsStatus::Closed
    );
}

#[test]
fn test_outgoing_set_remote_max_debt() {
    let currency = Currency::try_from("OFFST".to_owned()).unwrap();

    let local_public_key = PublicKey::from(&[0xaa; PublicKey::len()]);
    let remote_public_key = PublicKey::from(&[0xbb; PublicKey::len()]);
    let balance = 0;
    let mut mutual_credit =
        MutualCredit::new(&local_public_key, &remote_public_key, &currency, balance);

    assert_eq!(mutual_credit.state().balance.remote_max_debt, 0);
    apply_outgoing(&mut mutual_credit, &FriendTcOp::SetRemoteMaxDebt(20)).unwrap();
    assert_eq!(mutual_credit.state().balance.remote_max_debt, 20);
}

#[test]
fn test_request_response_collect_send_funds() {
    let currency = Currency::try_from("OFFST".to_owned()).unwrap();

    let local_public_key = PublicKey::from(&[0xaa; PublicKey::len()]);
    let remote_public_key = PublicKey::from(&[0xbb; PublicKey::len()]);
    let balance = 0;
    let mut mutual_credit =
        MutualCredit::new(&local_public_key, &remote_public_key, &currency, balance);

    // -----[SetRemoteMaxDebt]------
    // -----------------------------
    // Make enough trust from remote side, so that we will be able to send credits:
    apply_incoming(&mut mutual_credit, FriendTcOp::SetRemoteMaxDebt(100)).unwrap();

    // -----[EnableRequests]--------
    // -----------------------------
    // Remote side should open his requests status:
    apply_incoming(&mut mutual_credit, FriendTcOp::EnableRequests).unwrap();

    // -----[RequestSendFunds]--------
    // -----------------------------
    let rng = DummyRandom::new(&[1u8]);
    let private_key = PrivateKey::rand_gen(&rng);
    let identity = SoftwareEd25519Identity::from_private_key(&private_key).unwrap();
    let public_key_c = identity.get_public_key();

    let request_id = Uid::from(&[3; Uid::len()]);
    let route = FriendsRoute {
        public_keys: vec![
            PublicKey::from(&[0xaa; PublicKey::len()]),
            PublicKey::from(&[0xbb; PublicKey::len()]),
            public_key_c.clone(),
        ],
    };
    let invoice_id = InvoiceId::from(&[0; InvoiceId::len()]);
    let src_plain_lock = PlainLock::from(&[1; PlainLock::len()]);

    let request_send_funds = RequestSendFundsOp {
        request_id: request_id.clone(),
        src_hashed_lock: src_plain_lock.hash_lock(),
        route,
        dest_payment: 10,
        total_dest_payment: 10,
        invoice_id,
        left_fees: 5,
    };

    let pending_transaction = create_pending_transaction(&request_send_funds);
    apply_outgoing(
        &mut mutual_credit,
        &FriendTcOp::RequestSendFunds(request_send_funds),
    )
    .unwrap();

    assert_eq!(mutual_credit.state().balance.balance, 0);
    assert_eq!(mutual_credit.state().balance.local_max_debt, 100);
    assert_eq!(mutual_credit.state().balance.remote_max_debt, 0);
    assert_eq!(mutual_credit.state().balance.local_pending_debt, 10 + 5);
    assert_eq!(mutual_credit.state().balance.remote_pending_debt, 0);

    // -----[ResponseSendFunds]--------
    // --------------------------------
    let rand_nonce = RandValue::from(&[5; RandValue::len()]);
    let dest_plain_lock = PlainLock::from(&[2; PlainLock::len()]);

    let mut response_send_funds = ResponseSendFundsOp {
        request_id: request_id.clone(),
        dest_hashed_lock: dest_plain_lock.hash_lock(),
        is_complete: false,
        rand_nonce: rand_nonce.clone(),
        signature: Signature::from(&[0; Signature::len()]),
    };

    let sign_buffer =
        create_response_signature_buffer(&currency, &response_send_funds, &pending_transaction);
    response_send_funds.signature = identity.sign(&sign_buffer);

    apply_incoming(
        &mut mutual_credit,
        FriendTcOp::ResponseSendFunds(response_send_funds),
    )
    .unwrap();

    // We expect that no changes to balance happened yet:
    assert_eq!(mutual_credit.state().balance.balance, 0);
    assert_eq!(mutual_credit.state().balance.local_max_debt, 100);
    assert_eq!(mutual_credit.state().balance.remote_max_debt, 0);
    assert_eq!(mutual_credit.state().balance.local_pending_debt, 10 + 5);
    assert_eq!(mutual_credit.state().balance.remote_pending_debt, 0);

    // -----[CollectSendFunds]--------
    // --------------------------------
    let collect_send_funds = CollectSendFundsOp {
        request_id,
        src_plain_lock,
        dest_plain_lock,
    };

    apply_incoming(
        &mut mutual_credit,
        FriendTcOp::CollectSendFunds(collect_send_funds),
    )
    .unwrap();

    // We expect that no changes to balance happened yet:
    assert_eq!(mutual_credit.state().balance.balance, -15);
    assert_eq!(mutual_credit.state().balance.local_max_debt, 100);
    assert_eq!(mutual_credit.state().balance.remote_max_debt, 0);
    assert_eq!(mutual_credit.state().balance.local_pending_debt, 0);
    assert_eq!(mutual_credit.state().balance.remote_pending_debt, 0);
}

#[test]
fn test_request_cancel_send_funds() {
    let currency = Currency::try_from("OFFST".to_owned()).unwrap();

    let rng = DummyRandom::new(&[1u8]);
    let private_key = PrivateKey::rand_gen(&rng);
    let identity = SoftwareEd25519Identity::from_private_key(&private_key).unwrap();
    let public_key_b = identity.get_public_key();

    let local_public_key = PublicKey::from(&[0xaa; PublicKey::len()]);
    let remote_public_key = public_key_b.clone();
    let balance = 0;
    let mut mutual_credit =
        MutualCredit::new(&local_public_key, &remote_public_key, &currency, balance);

    // -----[SetRemoteMaxDebt]------
    // -----------------------------
    // Make enough trust from remote side, so that we will be able to send credits:
    apply_incoming(&mut mutual_credit, FriendTcOp::SetRemoteMaxDebt(100)).unwrap();

    // -----[EnableRequests]--------
    // -----------------------------
    // Remote side should open his requests status:
    apply_incoming(&mut mutual_credit, FriendTcOp::EnableRequests).unwrap();

    // -----[RequestSendFunds]--------
    // -----------------------------
    let request_id = Uid::from(&[3; Uid::len()]);
    let route = FriendsRoute {
        public_keys: vec![
            PublicKey::from(&[0xaa; PublicKey::len()]),
            public_key_b.clone(),
            PublicKey::from(&[0xcc; PublicKey::len()]),
        ],
    };
    let invoice_id = InvoiceId::from(&[0; InvoiceId::len()]);
    let src_plain_lock = PlainLock::from(&[1; PlainLock::len()]);

    let request_send_funds = RequestSendFundsOp {
        request_id: request_id.clone(),
        src_hashed_lock: src_plain_lock.hash_lock(),
        route,
        dest_payment: 10,
        total_dest_payment: 10,
        invoice_id,
        left_fees: 5,
    };

    apply_outgoing(
        &mut mutual_credit,
        &FriendTcOp::RequestSendFunds(request_send_funds),
    )
    .unwrap();

    assert_eq!(mutual_credit.state().balance.balance, 0);
    assert_eq!(mutual_credit.state().balance.local_max_debt, 100);
    assert_eq!(mutual_credit.state().balance.remote_max_debt, 0);
    assert_eq!(mutual_credit.state().balance.local_pending_debt, 10 + 5);
    assert_eq!(mutual_credit.state().balance.remote_pending_debt, 0);

    // -----[CancelSendFunds]--------
    // ------------------------------
    let cancel_send_funds = CancelSendFundsOp { request_id };

    apply_incoming(
        &mut mutual_credit,
        FriendTcOp::CancelSendFunds(cancel_send_funds),
    )
    .unwrap();

    assert_eq!(mutual_credit.state().balance.balance, 0);
    assert_eq!(mutual_credit.state().balance.local_max_debt, 100);
    assert_eq!(mutual_credit.state().balance.remote_max_debt, 0);
    assert_eq!(mutual_credit.state().balance.local_pending_debt, 0);
    assert_eq!(mutual_credit.state().balance.remote_pending_debt, 0);
}

#[test]
fn test_request_response_cancel_send_funds() {
    let currency = Currency::try_from("OFFST".to_owned()).unwrap();

    let local_public_key = PublicKey::from(&[0xaa; PublicKey::len()]);
    let remote_public_key = PublicKey::from(&[0xbb; PublicKey::len()]);
    let balance = 0;
    let mut mutual_credit =
        MutualCredit::new(&local_public_key, &remote_public_key, &currency, balance);

    // -----[SetRemoteMaxDebt]------
    // -----------------------------
    // Make enough trust from remote side, so that we will be able to send credits:
    apply_incoming(&mut mutual_credit, FriendTcOp::SetRemoteMaxDebt(100)).unwrap();

    // -----[EnableRequests]--------
    // -----------------------------
    // Remote side should open his requests status:
    apply_incoming(&mut mutual_credit, FriendTcOp::EnableRequests).unwrap();

    // -----[RequestSendFunds]--------
    // -----------------------------
    let rng = DummyRandom::new(&[1u8]);
    let private_key = PrivateKey::rand_gen(&rng);
    let identity = SoftwareEd25519Identity::from_private_key(&private_key).unwrap();
    let public_key_c = identity.get_public_key();

    let request_id = Uid::from(&[3; Uid::len()]);
    let route = FriendsRoute {
        public_keys: vec![
            PublicKey::from(&[0xaa; PublicKey::len()]),
            PublicKey::from(&[0xbb; PublicKey::len()]),
            public_key_c.clone(),
        ],
    };
    let invoice_id = InvoiceId::from(&[0; InvoiceId::len()]);
    let src_plain_lock = PlainLock::from(&[1; PlainLock::len()]);

    let request_send_funds = RequestSendFundsOp {
        request_id: request_id.clone(),
        src_hashed_lock: src_plain_lock.hash_lock(),
        route,
        dest_payment: 10,
        total_dest_payment: 10,
        invoice_id,
        left_fees: 5,
    };

    let pending_transaction = create_pending_transaction(&request_send_funds);
    apply_outgoing(
        &mut mutual_credit,
        &FriendTcOp::RequestSendFunds(request_send_funds),
    )
    .unwrap();

    assert_eq!(mutual_credit.state().balance.balance, 0);
    assert_eq!(mutual_credit.state().balance.local_max_debt, 100);
    assert_eq!(mutual_credit.state().balance.remote_max_debt, 0);
    assert_eq!(mutual_credit.state().balance.local_pending_debt, 10 + 5);
    assert_eq!(mutual_credit.state().balance.remote_pending_debt, 0);

    // -----[ResponseSendFunds]--------
    // --------------------------------
    let rand_nonce = RandValue::from(&[5; RandValue::len()]);
    let dest_plain_lock = PlainLock::from(&[2; PlainLock::len()]);

    let mut response_send_funds = ResponseSendFundsOp {
        request_id: request_id.clone(),
        dest_hashed_lock: dest_plain_lock.hash_lock(),
        is_complete: true,
        rand_nonce: rand_nonce.clone(),
        signature: Signature::from(&[0; Signature::len()]),
    };

    let sign_buffer =
        create_response_signature_buffer(&currency, &response_send_funds, &pending_transaction);
    response_send_funds.signature = identity.sign(&sign_buffer);

    apply_incoming(
        &mut mutual_credit,
        FriendTcOp::ResponseSendFunds(response_send_funds),
    )
    .unwrap();

    // We expect that no changes to balance happened yet:
    assert_eq!(mutual_credit.state().balance.balance, 0);
    assert_eq!(mutual_credit.state().balance.local_max_debt, 100);
    assert_eq!(mutual_credit.state().balance.remote_max_debt, 0);
    assert_eq!(mutual_credit.state().balance.local_pending_debt, 10 + 5);
    assert_eq!(mutual_credit.state().balance.remote_pending_debt, 0);

    // -----[CancelSendFunds]--------
    // ------------------------------
    let cancel_send_funds = CancelSendFundsOp { request_id };

    apply_incoming(
        &mut mutual_credit,
        FriendTcOp::CancelSendFunds(cancel_send_funds),
    )
    .unwrap();

    assert_eq!(mutual_credit.state().balance.balance, 0);
    assert_eq!(mutual_credit.state().balance.local_max_debt, 100);
    assert_eq!(mutual_credit.state().balance.remote_max_debt, 0);
    assert_eq!(mutual_credit.state().balance.local_pending_debt, 0);
    assert_eq!(mutual_credit.state().balance.remote_pending_debt, 0);
}
