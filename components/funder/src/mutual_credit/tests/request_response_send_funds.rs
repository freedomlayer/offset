use std::convert::TryFrom;

use common::test_executor::TestExecutor;

use crypto::hash_lock::HashLock;
use crypto::identity::{Identity, SoftwareEd25519Identity};
use crypto::rand::RandGen;
use crypto::test_utils::DummyRandom;

use proto::crypto::{HashResult, HmacResult, PlainLock, PrivateKey, PublicKey, Signature, Uid};
use proto::funder::messages::{
    Currency, FriendTcOp, FriendsRoute, RequestSendFundsOp, ResponseSendFundsOp,
};
use signature::signature_buff::create_response_signature_buffer;

use crate::mutual_credit::tests::utils::MutualCredit;
use crate::mutual_credit::types::McTransaction;
use crate::types::create_pending_transaction;

use crate::mutual_credit::incoming::process_operations_list;
use crate::mutual_credit::outgoing::queue_operation;

async fn task_request_response_send_funds() {
    let currency = Currency::try_from("FST".to_owned()).unwrap();

    let local_public_key = PublicKey::from(&[0xaa; PublicKey::len()]);
    let remote_public_key = PublicKey::from(&[0xbb; PublicKey::len()]);
    let balance = 0;

    let mut mc_transaction = MutualCredit::new(&currency, balance);

    // -----[RequestSendFunds]--------
    // -----------------------------
    let mut rng = DummyRandom::new(&[1u8]);
    let private_key = PrivateKey::rand_gen(&mut rng);
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
    let invoice_hash = HashResult::from(&[0; HashResult::len()]);
    let src_plain_lock = PlainLock::from(&[1; PlainLock::len()]);
    let hmac = HmacResult::from(&[2; HmacResult::len()]);

    let request_send_funds = RequestSendFundsOp {
        request_id: request_id.clone(),
        src_hashed_lock: src_plain_lock.hash_lock(),
        route,
        dest_payment: 10,
        total_dest_payment: 10,
        invoice_hash,
        hmac,
        left_fees: 5,
    };

    let pending_transaction = create_pending_transaction(&request_send_funds);
    queue_operation(
        &mut mc_transaction,
        FriendTcOp::RequestSendFunds(request_send_funds),
        &currency,
        &local_public_key,
    )
    .await
    .unwrap();

    let mc_balance = mc_transaction.get_balance().await.unwrap();
    assert_eq!(mc_balance.balance, 0);
    assert_eq!(mc_balance.local_pending_debt, 10 + 5);
    assert_eq!(mc_balance.remote_pending_debt, 0);
    assert_eq!(mc_balance.in_fees, 0);
    assert_eq!(mc_balance.out_fees, 0);

    // -----[ResponseSendFunds]--------
    // --------------------------------
    let serial_num: u128 = 0;

    let mut response_send_funds = ResponseSendFundsOp {
        request_id: request_id.clone(),
        src_plain_lock: src_plain_lock.clone(),
        serial_num,
        signature: Signature::from(&[0; Signature::len()]),
    };

    let sign_buffer = create_response_signature_buffer(
        &currency,
        response_send_funds.clone(),
        &pending_transaction,
    );
    response_send_funds.signature = identity.sign(&sign_buffer);

    process_operations_list(
        &mut mc_transaction,
        vec![FriendTcOp::ResponseSendFunds(response_send_funds)],
        &currency,
        &remote_public_key,
        100,
    )
    .await
    .unwrap();

    // We expect that the balance has updated:
    let mc_balance = mc_transaction.get_balance().await.unwrap();
    assert_eq!(mc_balance.balance, -15);
    assert_eq!(mc_balance.local_pending_debt, 0);
    assert_eq!(mc_balance.remote_pending_debt, 0);
    assert_eq!(mc_balance.in_fees, 0);
    assert_eq!(mc_balance.out_fees, 5);
}

#[test]
fn test_request_response_send_funds() {
    let test_executor = TestExecutor::new();
    let res = test_executor.run(task_request_response_send_funds());
    assert!(res.is_output());
}
