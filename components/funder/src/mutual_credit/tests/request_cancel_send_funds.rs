use std::convert::TryFrom;

use common::test_executor::TestExecutor;

use crypto::hash_lock::HashLock;
use crypto::identity::{Identity, SoftwareEd25519Identity};
use crypto::rand::RandGen;
use crypto::test_utils::DummyRandom;

use proto::crypto::{HashResult, PlainLock, PrivateKey, PublicKey, Uid};
use proto::funder::messages::{CancelSendFundsOp, Currency, FriendTcOp, RequestSendFundsOp};

use crate::mutual_credit::tests::utils::MockMutualCredit;
use crate::mutual_credit::types::McDbClient;

use crate::mutual_credit::outgoing::queue_operation;
use crate::mutual_credit::tests::utils::process_operations_list;

async fn task_request_cancel_send_funds() {
    let currency = Currency::try_from("FST".to_owned()).unwrap();

    let mut rng = DummyRandom::new(&[1u8]);
    let private_key = PrivateKey::rand_gen(&mut rng);
    let identity = SoftwareEd25519Identity::from_private_key(&private_key).unwrap();
    let public_key_b = identity.get_public_key();

    let local_public_key = PublicKey::from(&[0xaa; PublicKey::len()]);
    let remote_public_key = public_key_b.clone();
    let balance = 0;
    let in_fees = 0.into();
    let out_fees = 0.into();
    let mut mc_transaction = MockMutualCredit::new(currency.clone(), balance, in_fees, out_fees);

    // -----[RequestSendFunds]--------
    // -----------------------------
    let request_id = Uid::from(&[3; Uid::len()]);
    let route = vec![
        PublicKey::from(&[0xaa; PublicKey::len()]),
        public_key_b.clone(),
        PublicKey::from(&[0xcc; PublicKey::len()]),
    ];
    let invoice_hash = HashResult::from(&[0; HashResult::len()]);
    let src_plain_lock = PlainLock::from(&[1; PlainLock::len()]);

    let request_send_funds = RequestSendFundsOp {
        request_id: request_id.clone(),
        src_hashed_lock: src_plain_lock.hash_lock(),
        route,
        dest_payment: 10,
        invoice_hash,
        left_fees: 5,
    };

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
    assert_eq!(mc_balance.in_fees, 0.into());
    assert_eq!(mc_balance.out_fees, 0.into());

    // -----[CancelSendFunds]--------
    // ------------------------------
    let cancel_send_funds = CancelSendFundsOp { request_id };

    process_operations_list(
        &mut mc_transaction,
        vec![FriendTcOp::CancelSendFunds(cancel_send_funds)],
        &currency,
        &remote_public_key,
        100,
    )
    .await
    .unwrap();

    let mc_balance = mc_transaction.get_balance().await.unwrap();
    assert_eq!(mc_balance.balance, 0);
    assert_eq!(mc_balance.local_pending_debt, 0);
    assert_eq!(mc_balance.remote_pending_debt, 0);
    assert_eq!(mc_balance.in_fees, 0.into());
    assert_eq!(mc_balance.out_fees, 0.into());
}

#[test]
fn test_request_cancel_send_funds() {
    let test_executor = TestExecutor::new();
    let res = test_executor.run(task_request_cancel_send_funds());
    assert!(res.is_output());
}
