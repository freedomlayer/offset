use std::convert::TryFrom;

use common::test_executor::TestExecutor;

use crypto::hash_lock::HashLock;
use crypto::identity::{Identity, SoftwareEd25519Identity};
use crypto::rand::RandGen;
use crypto::test_utils::DummyRandom;

use proto::crypto::{HashResult, PlainLock, PrivateKey, PublicKey, Signature, Uid};
use proto::funder::messages::Currency;
use signature::signature_buff::create_response_signature_buffer;

use crate::mutual_credit::outgoing::queue_operation;
use crate::mutual_credit::tests::utils::{process_operations_list, MockMutualCredit};
use crate::mutual_credit::types::{McDbClient, McOp, McRequest, McResponse};
use crate::mutual_credit::utils::{
    pending_transaction_from_mc_request, response_op_from_mc_response,
};

async fn task_request_response() {
    let currency = Currency::try_from("FST".to_owned()).unwrap();

    let local_public_key = PublicKey::from(&[0xaa; PublicKey::len()]);
    let remote_public_key = PublicKey::from(&[0xbb; PublicKey::len()]);
    let balance = 0;
    let in_fees = 0.into();
    let out_fees = 0.into();
    let mut mc_transaction = MockMutualCredit::new(currency.clone(), balance, in_fees, out_fees);

    // -----[RequestSendFunds]--------
    // -----------------------------
    let mut rng = DummyRandom::new(&[1u8]);
    let private_key = PrivateKey::rand_gen(&mut rng);
    let identity = SoftwareEd25519Identity::from_private_key(&private_key).unwrap();
    let public_key_c = identity.get_public_key();

    let request_id = Uid::from(&[3; Uid::len()]);
    let route = vec![
        PublicKey::from(&[0xaa; PublicKey::len()]),
        PublicKey::from(&[0xbb; PublicKey::len()]),
        public_key_c.clone(),
    ];
    let invoice_hash = HashResult::from(&[0; HashResult::len()]);
    let src_plain_lock = PlainLock::from(&[1; PlainLock::len()]);

    let request = McRequest {
        request_id: request_id.clone(),
        src_hashed_lock: src_plain_lock.hash_lock(),
        route,
        dest_payment: 10,
        invoice_hash,
        left_fees: 5,
    };

    let pending_transaction =
        pending_transaction_from_mc_request(request.clone(), currency.clone());
    queue_operation(
        &mut mc_transaction,
        McOp::Request(request),
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

    // -----[ResponseSendFunds]--------
    // --------------------------------
    let serial_num: u128 = 0;

    let mut response = McResponse {
        request_id: request_id.clone(),
        src_plain_lock: src_plain_lock.clone(),
        serial_num,
        signature: Signature::from(&[0; Signature::len()]),
    };

    let sign_buffer = create_response_signature_buffer(
        &currency,
        response_op_from_mc_response(response.clone()),
        &pending_transaction,
    );
    response.signature = identity.sign(&sign_buffer);

    process_operations_list(
        &mut mc_transaction,
        vec![McOp::Response(response)],
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
    assert_eq!(mc_balance.in_fees, 0.into());
    assert_eq!(mc_balance.out_fees, 5.into());
}

#[test]
fn test_request_response() {
    let test_executor = TestExecutor::new();
    let res = test_executor.run(task_request_response());
    assert!(res.is_output());
}
