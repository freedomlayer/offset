use std::convert::TryFrom;

use futures::task::SpawnExt;
use futures::{future, FutureExt};

use common::test_executor::TestExecutor;

use crypto::hash_lock::HashLock;
use crypto::identity::{Identity, SoftwareEd25519Identity};
use crypto::rand::RandGen;
use crypto::test_utils::DummyRandom;

use signature::signature_buff::create_response_signature_buffer;

use proto::crypto::{
    HashResult, HashedLock, HmacResult, PlainLock, PrivateKey, PublicKey, Signature, Uid,
};
use proto::funder::messages::{
    Currency, CurrencyOperations, FriendTcOp, FriendsRoute, RequestSendFundsOp, ResponseSendFundsOp,
};

use identity::{create_identity, IdentityClient};

use crate::mutual_credit::incoming::IncomingMessage;
use crate::token_channel::tests::utils::MockTokenChannel;
use crate::token_channel::{
    accept_remote_reset, handle_in_move_token, handle_out_move_token, reset_balance_to_mc_balance,
    MoveTokenReceived, ReceiveMoveTokenOutput, TcClient, TcStatus, TokenChannelError,
};
use crate::types::create_pending_transaction;

async fn task_move_token_basic(test_executor: TestExecutor) {
    let currency1 = Currency::try_from("FST1".to_owned()).unwrap();
    let currency2 = Currency::try_from("FST2".to_owned()).unwrap();
    let currency3 = Currency::try_from("FST3".to_owned()).unwrap();

    let mut rng_a = DummyRandom::new(&[0xau8]);
    let pkcs8 = PrivateKey::rand_gen(&mut rng_a);
    let identity_a = SoftwareEd25519Identity::from_private_key(&pkcs8).unwrap();
    let pk_a = identity_a.get_public_key();

    let mut rng_b = DummyRandom::new(&[0xbu8]);
    let pkcs8 = PrivateKey::rand_gen(&mut rng_b);
    let identity_b = SoftwareEd25519Identity::from_private_key(&pkcs8).unwrap();
    let pk_b = identity_b.get_public_key();

    let mut tc_a_b = MockTokenChannel::<u32>::new(&pk_a, &pk_b);
    let mut tc_b_a = MockTokenChannel::<u32>::new(&pk_b, &pk_a);

    // Sort `a` and `b` entities, to have always have `a` as the first sender.
    let (pk_a, pk_b, identity_a, identity_b, mut tc_a_b, mut tc_b_a) =
        match tc_a_b.get_tc_status().await.unwrap() {
            TcStatus::ConsistentOut(..) => (pk_a, pk_b, identity_a, identity_b, tc_a_b, tc_b_a),
            TcStatus::ConsistentIn(..) => (pk_b, pk_a, identity_b, identity_a, tc_b_a, tc_a_b),
            TcStatus::Inconsistent(..) => unreachable!(),
        };

    // Spawn identity servers:
    let (requests_sender_a, identity_server_a) = create_identity(identity_a);
    let mut identity_client_a = IdentityClient::new(requests_sender_a);
    test_executor
        .spawn(identity_server_a.then(|_| future::ready(())))
        .unwrap();

    let (requests_sender_b, identity_server_b) = create_identity(identity_b);
    let mut identity_client_b = IdentityClient::new(requests_sender_b);
    test_executor
        .spawn(identity_server_b.then(|_| future::ready(())))
        .unwrap();

    // Send a MoveToken message from b to a, adding a currency:
    // --------------------------------------------------------
    let currencies_operations = Vec::new();
    let relays_diff = Vec::new();
    let currencies_diff = vec![currency1.clone()];
    let move_token = handle_out_move_token(
        &mut tc_b_a,
        &mut identity_client_b,
        currencies_operations,
        relays_diff,
        currencies_diff,
        &pk_b,
        &pk_a,
    )
    .await
    .unwrap();

    // Receive the MoveToken message at a:
    // -----------------------------------
    assert!(matches!(
        handle_in_move_token(
            &mut tc_a_b,
            &mut identity_client_a,
            move_token,
            &pk_a,
            &pk_b,
        )
        .await,
        Ok(ReceiveMoveTokenOutput::Received(_))
    ));

    // Send a MoveToken message from a to b, adding two currencies:
    // ------------------------------------------------------------
    let currencies_operations = Vec::new();
    let relays_diff = Vec::new();
    let currencies_diff = vec![currency1.clone(), currency2.clone()];
    let move_token = handle_out_move_token(
        &mut tc_a_b,
        &mut identity_client_a,
        currencies_operations,
        relays_diff,
        currencies_diff,
        &pk_a,
        &pk_b,
    )
    .await
    .unwrap();

    // Receive the MoveToken message at b:
    // -----------------------------------
    assert!(matches!(
        handle_in_move_token(
            &mut tc_b_a,
            &mut identity_client_b,
            move_token,
            &pk_b,
            &pk_a,
        )
        .await,
        Ok(ReceiveMoveTokenOutput::Received(_))
    ));

    // Send a MoveToken message from b to a, sending a request send funds message:
    // ---------------------------------------------------------------------------
    let src_plain_lock = PlainLock::from(&[0xaa; PlainLock::len()]);
    let request_send_funds_op = RequestSendFundsOp {
        request_id: Uid::from(&[0; Uid::len()]),
        src_hashed_lock: src_plain_lock.hash_lock(),
        route: FriendsRoute {
            public_keys: vec![pk_a.clone()],
        },
        dest_payment: 20u128,
        total_dest_payment: 30u128,
        invoice_hash: HashResult::from(&[0; HashResult::len()]),
        hmac: HmacResult::from(&[0; HmacResult::len()]),
        left_fees: 5u128,
    };
    let pending_transaction = create_pending_transaction(&request_send_funds_op);
    let currencies_operations = vec![CurrencyOperations {
        currency: currency1.clone(),
        operations: vec![FriendTcOp::RequestSendFunds(request_send_funds_op.clone())],
    }];
    let relays_diff = Vec::new();
    let currencies_diff = vec![];
    let move_token = handle_out_move_token(
        &mut tc_b_a,
        &mut identity_client_b,
        currencies_operations,
        relays_diff,
        currencies_diff,
        &pk_b,
        &pk_a,
    )
    .await
    .unwrap();

    // Add a remote max debt to a, so that `a` will be able to receive credits from `b`:
    // ---------------------------------------------------------------------------------
    tc_a_b.remote_max_debts.insert(currency1.clone(), 100u128);

    // Receive the MoveToken message at a:
    // ----------------------------------
    let res = handle_in_move_token(
        &mut tc_a_b,
        &mut identity_client_a,
        move_token,
        &pk_a,
        &pk_b,
    )
    .await
    .unwrap();

    // Make sure the the result is as expected:
    let mut currencies = match res {
        ReceiveMoveTokenOutput::Received(MoveTokenReceived {
            currencies,
            relays_diff,
        }) => {
            assert!(relays_diff.is_empty());
            currencies
        }
        _ => unreachable!(),
    };

    assert_eq!(currencies.len(), 1);
    let mut move_token_received_currency = currencies.pop().unwrap();
    assert_eq!(move_token_received_currency.currency, currency1);
    assert_eq!(move_token_received_currency.incoming_messages.len(), 1);
    let incoming_message = move_token_received_currency
        .incoming_messages
        .pop()
        .unwrap();
    let received_request_send_funds_op = match incoming_message {
        IncomingMessage::Request(request_send_funds_op) => request_send_funds_op,
        _ => unreachable!(),
    };
    assert_eq!(received_request_send_funds_op, request_send_funds_op);

    // Send a MoveToken message from a to b, sending a response send funds message:
    // ----------------------------------------------------------------------------
    let mut response_send_funds_op = ResponseSendFundsOp {
        // Matches earlier's request_id:
        request_id: Uid::from(&[0; Uid::len()]),
        src_plain_lock,
        serial_num: 0,
        // Temporary signature value, calculated later:
        signature: Signature::from(&[0; Signature::len()]),
    };

    // Sign the response:
    let sign_buffer = create_response_signature_buffer(
        &currency1,
        response_send_funds_op.clone(),
        &pending_transaction,
    );
    response_send_funds_op.signature = identity_client_a
        .request_signature(sign_buffer)
        .await
        .unwrap();

    let currencies_operations = vec![CurrencyOperations {
        currency: currency1.clone(),
        operations: vec![FriendTcOp::ResponseSendFunds(
            response_send_funds_op.clone(),
        )],
    }];
    let relays_diff = Vec::new();
    let currencies_diff = vec![];
    let move_token = handle_out_move_token(
        &mut tc_a_b,
        &mut identity_client_a,
        currencies_operations,
        relays_diff,
        currencies_diff,
        &pk_a,
        &pk_b,
    )
    .await
    .unwrap();

    // Insert remote_max_debt for currency1, to allow handling incoming operations for this currency.
    // ---------------------------------------------------------------------------------------------
    tc_b_a.remote_max_debts.insert(currency1.clone(), 20u128);

    // Receive the MoveToken message at b:
    // ----------------------------------
    let res = handle_in_move_token(
        &mut tc_b_a,
        &mut identity_client_b,
        move_token,
        &pk_b,
        &pk_a,
    )
    .await
    .unwrap();
}

#[test]
fn test_move_token_basic() {
    let test_executor = TestExecutor::new();
    let res = test_executor.run(task_move_token_basic(test_executor.clone()));
    assert!(res.is_output());
}
