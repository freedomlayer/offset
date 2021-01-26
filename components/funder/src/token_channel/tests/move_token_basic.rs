use std::convert::TryFrom;

use futures::task::SpawnExt;
use futures::{future, FutureExt, StreamExt};

use common::test_executor::TestExecutor;

use crypto::hash_lock::HashLock;
use crypto::identity::{Identity, SoftwareEd25519Identity};
use crypto::rand::RandGen;
use crypto::test_utils::DummyRandom;

use signature::signature_buff::create_response_signature_buffer;

use proto::crypto::{
    HashResult, HashedLock, HmacResult, PlainLock, PrivateKey, PublicKey, Signature, Uid,
};
use proto::funder::messages::{Currency, FriendTcOp, RequestSendFundsOp, ResponseSendFundsOp};

use identity::{create_identity, IdentityClient};

use crate::mutual_credit::incoming::IncomingMessage;
use crate::mutual_credit::types::{McRequest, McResponse};

use crate::token_channel::tests::utils::MockTokenChannel;
use crate::token_channel::types::TcCurrencyConfig;
use crate::token_channel::{
    accept_remote_reset, handle_in_move_token, reset_balance_to_mc_balance, MoveTokenReceived,
    OutMoveToken, ReceiveMoveTokenOutput, TcDbClient, TcStatus, TokenChannelError,
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

    let mut tc_a_b = MockTokenChannel::new(&pk_a, &pk_b);
    let mut tc_b_a = MockTokenChannel::new(&pk_b, &pk_a);

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

    /*
    // Send a MoveToken message from b to a, adding a currency:
    // --------------------------------------------------------
    // let currencies_operations = Vec::new();
    // let currencies_diff = vec![currency1.clone()];

    let out_move_token = OutMoveToken::new();
    let move_token = out_move_token
        .finalize(&mut tc_b_a, &mut identity_client_b, &pk_b, &pk_a)
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
    let currencies_diff = vec![currency1.clone(), currency2.clone()];
    let move_token = handle_out_move_token(
        &mut tc_a_b,
        &mut identity_client_a,
        currencies_operations,
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
    */

    // Add currency config at b, to be able to send a request to a:
    // ---------------------------------------------------------------------------------
    tc_b_a.currency_configs.insert(
        currency1.clone(),
        TcCurrencyConfig {
            local_max_debt: u128::MAX,
            remote_max_debt: 0,
        },
    );

    // Send a MoveToken message from b to a, sending a request send funds message:
    // ---------------------------------------------------------------------------
    let src_plain_lock = PlainLock::from(&[0xaa; PlainLock::len()]);
    let mc_request = McRequest {
        request_id: Uid::from(&[0; Uid::len()]),
        src_hashed_lock: src_plain_lock.clone().hash_lock(),
        dest_payment: 20u128,
        invoice_hash: HashResult::from(&[0; HashResult::len()]),
        route: vec![pk_a.clone()],
        left_fees: 5u128,
    };

    // TODO: How can this be done more elegantly?
    let pending_transaction = {
        let request_send_funds_op = RequestSendFundsOp {
            request_id: mc_request.request_id.clone(),
            currency: currency1.clone(),
            src_hashed_lock: mc_request.src_hashed_lock.clone(),
            dest_payment: mc_request.dest_payment.clone(),
            invoice_hash: mc_request.invoice_hash.clone(),
            route: mc_request.route.clone(),
            left_fees: mc_request.left_fees,
        };
        create_pending_transaction(&request_send_funds_op)
    };

    let mut out_move_token = OutMoveToken::new();
    let _mc_balance = out_move_token
        .queue_request(&mut tc_b_a, currency1.clone(), mc_request.clone())
        .await
        .unwrap();
    let move_token = out_move_token
        .finalize(&mut tc_b_a, &mut identity_client_b, &pk_b, &pk_a)
        .await
        .unwrap();

    /*
    let move_token = handle_out_move_token(
        &mut tc_b_a,
        &mut identity_client_b,
        currencies_operations,
        currencies_diff,
        &pk_b,
        &pk_a,
    )
    .await
    .unwrap();
    */

    {
        // Assert balances:
        let mut b_balances_iter = tc_b_a.list_balances();
        let mut b_balances = Vec::new();
        while let Some(item) = b_balances_iter.next().await {
            b_balances.push(item.unwrap());
        }
        assert_eq!(b_balances.len(), 1);
        let (currency, mc_balance) = b_balances.pop().unwrap();
        assert_eq!(currency, currency1);
        assert_eq!(mc_balance.balance, 0);
        assert_eq!(mc_balance.local_pending_debt, 25);
        assert_eq!(mc_balance.remote_pending_debt, 0);
        assert_eq!(mc_balance.in_fees, 0.into());
        assert_eq!(mc_balance.out_fees, 0.into());
    }

    // Add a remote max debt to a, so that `a` will be able to receive credits from `b`:
    // ---------------------------------------------------------------------------------
    tc_a_b.currency_configs.insert(
        currency1.clone(),
        TcCurrencyConfig {
            local_max_debt: u128::MAX,
            remote_max_debt: 25u128, // 20 credits payment + 5 credits fees
        },
    );

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
    let mut incoming_messages = match res {
        ReceiveMoveTokenOutput::Received(MoveTokenReceived { incoming_messages }) => {
            incoming_messages
        }
        _ => unreachable!(),
    };

    assert_eq!(incoming_messages.len(), 1);
    let (currency, incoming_message) = incoming_messages.pop().unwrap();
    assert_eq!(currency, currency1);
    let received_mc_request = match incoming_message {
        IncomingMessage::Request(received_mc_request) => received_mc_request,
        _ => unreachable!(),
    };
    assert_eq!(received_mc_request, mc_request);

    // Send a MoveToken message from a to b, sending a response send funds message:
    // ----------------------------------------------------------------------------

    // TODO: How to do this more elegantly?
    let mc_response = {
        let mut mc_response = McResponse {
            request_id: Uid::from(&[0; Uid::len()]),
            src_plain_lock,
            serial_num: 0,
            // Temporary signature value, calculated later:
            signature: Signature::from(&[0; Signature::len()]),
        };
        let response_send_funds_op = ResponseSendFundsOp {
            request_id: mc_response.request_id.clone(),
            src_plain_lock: mc_response.src_plain_lock.clone(),
            serial_num: mc_response.serial_num.clone(),
            signature: mc_response.signature.clone(),
        };

        // Sign the response:
        let sign_buffer = create_response_signature_buffer(
            &currency1,
            response_send_funds_op.clone(),
            &pending_transaction,
        );
        mc_response.signature = identity_client_a
            .request_signature(sign_buffer)
            .await
            .unwrap();
        mc_response
    };

    let mut out_move_token = OutMoveToken::new();
    let _mc_balance = out_move_token
        .queue_response(&mut tc_a_b, currency1.clone(), mc_response, &pk_a)
        .await
        .unwrap();
    let move_token = out_move_token
        .finalize(&mut tc_a_b, &mut identity_client_a, &pk_a, &pk_b)
        .await
        .unwrap();

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

    {
        // Assert balances (b):
        let mut b_balances_iter = tc_b_a.list_balances();
        let mut b_balances = Vec::new();
        while let Some(item) = b_balances_iter.next().await {
            b_balances.push(item.unwrap());
        }
        assert_eq!(b_balances.len(), 1);
        let (currency, mc_balance) = b_balances.pop().unwrap();
        assert_eq!(currency, currency1);
        assert_eq!(mc_balance.balance, -25);
        assert_eq!(mc_balance.local_pending_debt, 0);
        assert_eq!(mc_balance.remote_pending_debt, 0);
        assert_eq!(mc_balance.in_fees, 0.into());
        assert_eq!(mc_balance.out_fees, 5.into());

        // Assert balances (a):
        let mut a_balances_iter = tc_a_b.list_balances();
        let mut a_balances = Vec::new();
        while let Some(item) = a_balances_iter.next().await {
            a_balances.push(item.unwrap());
        }
        assert_eq!(a_balances.len(), 1);
        let (currency, mc_balance) = a_balances.pop().unwrap();
        assert_eq!(currency, currency1);
        assert_eq!(mc_balance.balance, 25);
        assert_eq!(mc_balance.local_pending_debt, 0);
        assert_eq!(mc_balance.remote_pending_debt, 0);
        assert_eq!(mc_balance.in_fees, 5.into());
        assert_eq!(mc_balance.out_fees, 0.into());
    }
}

#[test]
fn test_move_token_basic() {
    let test_executor = TestExecutor::new();
    let res = test_executor.run(task_move_token_basic(test_executor.clone()));
    assert!(res.is_output());
}
