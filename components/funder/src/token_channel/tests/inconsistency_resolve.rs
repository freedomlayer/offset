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
use proto::funder::messages::{
    Currency, CurrencyOperations, FriendTcOp, FriendsRoute, RequestSendFundsOp, ResponseSendFundsOp,
};

use identity::{create_identity, IdentityClient};

use crate::mutual_credit::incoming::IncomingMessage;
use crate::token_channel::tests::utils::MockTokenChannel;
use crate::token_channel::{
    accept_remote_reset, handle_in_move_token, handle_out_move_token, load_remote_reset_terms,
    reset_balance_to_mc_balance, MoveTokenReceived, ReceiveMoveTokenOutput, ResetBalance, TcClient,
    TcStatus, TokenChannelError,
};
use crate::types::create_pending_transaction;

async fn task_inconsistency_resolve(test_executor: TestExecutor) {
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

    // Send a MoveToken message from b to a, adding a currency:
    // --------------------------------------------------------
    let currencies_operations = Vec::new();
    let currencies_diff = vec![currency1.clone()];
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

    // Assert current counter value:
    assert_eq!(tc_a_b.get_move_token_counter().await.unwrap(), 1);
    assert_eq!(tc_b_a.get_move_token_counter().await.unwrap(), 1);

    // Send a MoveToken message from a to b, adding two currencies,
    // and set incorrect token info hash.
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

    assert_eq!(tc_a_b.get_move_token_counter().await.unwrap(), 2);
    assert_eq!(tc_b_a.get_move_token_counter().await.unwrap(), 1);

    // b is reset, this is done to simulate inconsistency:
    // ---------------------------------------------------
    let mut tc_b_a = MockTokenChannel::new(&pk_b, &pk_a);
    assert_eq!(tc_b_a.get_move_token_counter().await.unwrap(), 0);

    // Receive the MoveToken message at b:
    // -----------------------------------
    let res = handle_in_move_token(
        &mut tc_b_a,
        &mut identity_client_b,
        move_token,
        &pk_b,
        &pk_a,
    )
    .await
    .unwrap();

    let reset_terms_b = match res {
        ReceiveMoveTokenOutput::ChainInconsistent(reset_terms) => {
            assert_eq!(reset_terms.move_token_counter, 0 + 2);
            assert!(reset_terms.reset_balances.is_empty());
            reset_terms
        }
        _ => unreachable!(),
    };

    // Set a to be inconsistent, and get a's reset terms:
    // -------------------------------------------------
    let reset_terms_a = load_remote_reset_terms(
        &mut tc_a_b,
        &mut identity_client_a,
        reset_terms_b,
        &pk_a,
        &pk_b,
    )
    .await
    .unwrap()
    .unwrap();

    // Assert a's reset terms:
    assert_eq!(reset_terms_a.move_token_counter, 2 + 2);
    assert_eq!(reset_terms_a.reset_balances.len(), 1);
    let expected_reset_balance = ResetBalance {
        balance: 0,
        in_fees: 0.into(),
        out_fees: 0.into(),
    };
    assert_eq!(
        reset_terms_a.reset_balances.get(&currency1),
        Some(&expected_reset_balance)
    );

    // b: load a's reset terms:
    // ------------------------
    assert!(matches!(
        load_remote_reset_terms(
            &mut tc_b_a,
            &mut identity_client_b,
            reset_terms_a,
            &pk_b,
            &pk_a,
        )
        .await
        .unwrap(),
        None
    ));

    // b accepts a's reset terms:
    // --------------------------
    let currencies_operations = Vec::new();
    let currencies_diff = Vec::new();
    let move_token = accept_remote_reset(
        &mut tc_b_a,
        &mut identity_client_b,
        currencies_operations,
        currencies_diff,
        &pk_b,
        &pk_a,
    )
    .await
    .unwrap();

    // a: receive reset move token message:
    // -----------------------------------
    let res = handle_in_move_token(
        &mut tc_a_b,
        &mut identity_client_a,
        move_token,
        &pk_a,
        &pk_b,
    )
    .await
    .unwrap();

    // TODO: Send an extra move token between a and b, just ot make sure channel is now consistent.
}

#[test]
fn test_inconsistency_resolve() {
    let test_executor = TestExecutor::new();
    let res = test_executor.run(task_inconsistency_resolve(test_executor.clone()));
    assert!(res.is_output());
}
