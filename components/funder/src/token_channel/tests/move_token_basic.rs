use std::convert::TryFrom;

use futures::task::SpawnExt;
use futures::{future, FutureExt};

use common::test_executor::TestExecutor;

use crypto::identity::{Identity, SoftwareEd25519Identity};
use crypto::rand::RandGen;
use crypto::test_utils::DummyRandom;

use proto::crypto::{PrivateKey, PublicKey};
use proto::funder::messages::Currency;

use identity::{create_identity, IdentityClient};

use crate::token_channel::tests::utils::MockTokenChannel;
use crate::token_channel::{
    accept_remote_reset, handle_in_move_token, handle_out_move_token, reset_balance_to_mc_balance,
    TcClient, TcStatus, TokenChannelError,
};

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
    handle_in_move_token(
        &mut tc_a_b,
        &mut identity_client_a,
        move_token,
        &pk_a,
        &pk_b,
    )
    .await
    .unwrap();

    // Send a MoveToken message from a to b, adding two currencies:
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
    handle_in_move_token(
        &mut tc_b_a,
        &mut identity_client_a,
        move_token,
        &pk_b,
        &pk_a,
    )
    .await
    .unwrap();

    // TODO: Continue here.
}

#[test]
fn test_move_token_basic() {
    let test_executor = TestExecutor::new();
    let res = test_executor.run(task_move_token_basic(test_executor.clone()));
    assert!(res.is_output());
}
