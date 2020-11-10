use std::convert::TryFrom;

use common::test_executor::TestExecutor;

use crypto::identity::{Identity, SoftwareEd25519Identity};
use crypto::rand::RandGen;
use crypto::test_utils::DummyRandom;

use proto::crypto::{PrivateKey, PublicKey};
use proto::funder::messages::Currency;

use crate::token_channel::tests::utils::MockTokenChannel;
use crate::token_channel::{
    accept_remote_reset, handle_in_move_token, handle_out_move_token, reset_balance_to_mc_balance,
    TcClient, TcStatus, TokenChannelError,
};

async fn task_move_token_basic(test_executor: TestExecutor) {
    let currency = Currency::try_from("FST".to_owned()).unwrap();

    let mut rng_a = DummyRandom::new(&[0xau8]);
    let pkcs8 = PrivateKey::rand_gen(&mut rng_a);
    let identity_a = SoftwareEd25519Identity::from_private_key(&pkcs8).unwrap();

    let mut rng_b = DummyRandom::new(&[0xbu8]);
    let pkcs8 = PrivateKey::rand_gen(&mut rng_b);
    let identity_b = SoftwareEd25519Identity::from_private_key(&pkcs8).unwrap();

    let pk_a = PublicKey::from(&[0xaa; PublicKey::len()]);
    let pk_b = PublicKey::from(&[0xbb; PublicKey::len()]);
    let mut tc_a_b = MockTokenChannel::<u32>::new(&pk_a, &pk_b);
    let mut tc_b_a = MockTokenChannel::<u32>::new(&pk_b, &pk_a);

    // Sort `a` and `b` entities, to have always have `a` as the first sender.
    let (pk_a, pk_b, identity_a, identity_b, tc_a_b, tc_b_a) =
        match tc_a_b.get_tc_status().await.unwrap() {
            TcStatus::ConsistentOut(..) => (pk_a, pk_b, identity_a, identity_b, tc_a_b, tc_b_a),
            TcStatus::ConsistentIn(..) => (pk_b, pk_a, identity_b, identity_a, tc_b_a, tc_a_b),
            TcStatus::Inconsistent(..) => unreachable!(),
        };

    /*
    let currencies_operations = Vec::new();
    let relays_diff = Vec::new();
    let currencies_diff = vec![currency];
    handle_out_move_token(&mut tc_a_b, identity_a, currencies_operations, relays_diff
    */
}

#[test]
fn test_move_token_basic() {
    let test_executor = TestExecutor::new();
    let res = test_executor.run(task_move_token_basic(test_executor.clone()));
    assert!(res.is_output());
}
