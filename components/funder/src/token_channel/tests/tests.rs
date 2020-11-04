use common::test_executor::TestExecutor;

use proto::crypto::PrivateKey;
use proto::funder::messages::MoveToken;
// use proto::funder::messages::FriendTcOp;

use crypto::identity::{Identity, SoftwareEd25519Identity};
use crypto::rand::RandGen;
use crypto::test_utils::DummyRandom;

use signature::signature_buff::move_token_signature_buff;

/*
async fn task_initial_direction(executor: TestExecutor) {
    init_move_token

    init_token_channel(
        tc_client: tc_client,
        local_public_key,
        remote_public_key).await;

}
*/

#[test]
fn test_initial_direction() {
    let test_executor = TestExecutor::new();
    let res = test_executor.run(task_initial_direction(test_executor));
    assert!(res.is_output());
}

#[test]
fn test_initial_direction() {
    let pk_a = PublicKey::from(&[0xaa; PublicKey::len()]);
    let pk_b = PublicKey::from(&[0xbb; PublicKey::len()]);
    let token_channel_a_b = TokenChannel::<u32>::new(&pk_a, &pk_b);
    let token_channel_b_a = TokenChannel::<u32>::new(&pk_b, &pk_a);

    // Only one of those token channels is outgoing:
    let is_a_b_outgoing = token_channel_a_b.get_outgoing().is_some();
    let is_b_a_outgoing = token_channel_b_a.get_outgoing().is_some();
    assert!(is_a_b_outgoing ^ is_b_a_outgoing);

    let (out_tc, in_tc) = if is_a_b_outgoing {
        (token_channel_a_b, token_channel_b_a)
    } else {
        (token_channel_b_a, token_channel_a_b)
    };

    let out_hashed = match &out_tc.direction {
        TcDirection::Incoming(_) => unreachable!(),
        TcDirection::Outgoing(outgoing) => {
            create_hashed(&outgoing.move_token_out, &outgoing.token_info)
        }
    };

    let in_hashed = match &in_tc.direction {
        TcDirection::Outgoing(_) => unreachable!(),
        TcDirection::Incoming(incoming) => &incoming.move_token_in,
    };

    assert_eq!(&out_hashed, in_hashed);

    // assert_eq!(create_hashed(&out_tc.move_token_out), in_tc.move_token_in);
    // assert_eq!(out_tc.get_cur_move_token_hashed(), in_tc.get_cur_move_token_hashed());
    let tc_outgoing = match out_tc.get_direction() {
        TcDirectionBorrow::Out(tc_out_borrow) => tc_out_borrow.tc_outgoing,
        TcDirectionBorrow::In(_) => unreachable!(),
    };
    assert!(tc_outgoing.opt_prev_move_token_in.is_none());
}

/// Sort the two identity client.
/// The result will be a pair where the first is initially configured to have outgoing message,
/// and the second is initially configured to have incoming message.
fn sort_sides<I>(identity1: I, identity2: I) -> (I, I)
where
    I: Identity,
{
    let pk1 = identity1.get_public_key();
    let pk2 = identity2.get_public_key();
    let token_channel12 = TokenChannel::<u32>::new(&pk1, &pk2); // (local, remote)
    if token_channel12.get_outgoing().is_some() {
        (identity1, identity2)
    } else {
        (identity2, identity1)
    }
}
/// Before: tc1: outgoing, tc2: incoming
/// Send AddCurrency: tc2 -> tc1
/// After: tc1: incoming, tc2: outgoing
fn add_currencies<I>(
    _identity1: &I,
    identity2: &I,
    tc1: &mut TokenChannel<u32>,
    tc2: &mut TokenChannel<u32>,
    currencies: &[Currency],
) where
    I: Identity,
{
    assert!(tc1.get_outgoing().is_some());
    assert!(!tc2.get_outgoing().is_some());

    let tc2_in_borrow = tc2.get_incoming().unwrap();
    let currencies_operations = vec![];

    let rand_nonce = RandValue::from(&[7; RandValue::len()]);
    let opt_local_relays = None;
    let opt_active_currencies = Some(currencies.to_vec());

    let SendMoveTokenOutput {
        unsigned_move_token,
        mutations,
        token_info,
    } = tc2_in_borrow
        .simulate_send_move_token(
            currencies_operations,
            opt_local_relays,
            opt_active_currencies,
            rand_nonce,
        )
        .unwrap();

    for tc_mutation in mutations {
        tc2.mutate(&tc_mutation);
    }

    // This is the only mutation we can not produce from inside TokenChannel, because it
    // requires a signature:
    let friend_move_token = dummy_sign_move_token(unsigned_move_token, identity2);
    let tc_mutation = TcMutation::SetDirection(SetDirection::Outgoing((
        friend_move_token.clone(),
        token_info.clone(),
    )));
    tc2.mutate(&tc_mutation);

    assert!(tc2.get_outgoing().is_some());

    let receive_move_token_output = tc1
        .simulate_receive_move_token(friend_move_token.clone(), &ImHashMap::new())
        .unwrap();

    let move_token_received = match receive_move_token_output {
        ReceiveMoveTokenOutput::Received(move_token_received) => move_token_received,
        _ => unreachable!(),
    };

    assert!(move_token_received.currencies.is_empty());

    // let mut seen_mc_mutation = false;
    // let mut seen_set_direction = false;

    for tc_mutation in &move_token_received.mutations {
        tc1.mutate(tc_mutation);
    }

    assert!(!tc1.get_outgoing().is_some());
    match &tc1.direction {
        TcDirection::Outgoing(_) => unreachable!(),
        TcDirection::Incoming(tc_incoming) => {
            assert_eq!(
                tc_incoming.move_token_in,
                create_hashed(&friend_move_token, &token_info)
            );
        }
    };

    for currency in currencies {
        assert!(tc2.active_currencies.local.contains(currency));
        assert!(tc1.active_currencies.remote.contains(currency));
    }
}

/*
/// Before: tc1: outgoing, tc2: incoming
/// Send SetRemoteMaxDebt: tc2 -> tc1
/// After: tc1: incoming, tc2: outgoing
fn set_remote_max_debt21<I>(
    _identity1: &I,
    identity2: &I,
    tc1: &mut TokenChannel<u32>,
    tc2: &mut TokenChannel<u32>,
    currency: &Currency,
) where
    I: Identity,
{
    assert!(tc1.get_outgoing().is_some());
    assert!(tc2.get_incoming().is_some());

    let tc2_in_borrow = tc2.get_incoming().unwrap();
    // let mut outgoing_mc = tc2_in_borrow.create_outgoing_mc(&currency).unwrap();
    //
    // let friend_tc_op = FriendTcOp::SetRemoteMaxDebt(100);
    //
    // let mc_mutations = outgoing_mc.queue_operation(&friend_tc_op).unwrap();
    let currency_operations = CurrencyOperations {
        currency: currency.clone(),
        operations: vec![friend_tc_op],
    };
    let currencies_operations = vec![currency_operations];

    let rand_nonce = RandValue::from(&[5; RandValue::len()]);
    let opt_local_relays = None;
    let opt_active_currencies = None;

    let SendMoveTokenOutput {
        unsigned_move_token,
        mutations,
        token_info,
    } = tc2_in_borrow
        .simulate_send_move_token(
            currencies_operations,
            opt_local_relays,
            opt_active_currencies,
            rand_nonce,
        )
        .unwrap();

    for tc_mutation in mutations {
        tc2.mutate(&tc_mutation);
    }

    let friend_move_token = dummy_sign_move_token(unsigned_move_token, identity2);
    let tc_mutation = TcMutation::SetDirection(SetDirection::Outgoing((
        friend_move_token.clone(),
        token_info.clone(),
    )));
    tc2.mutate(&tc_mutation);

    assert!(tc2.get_outgoing().is_some());

    let receive_move_token_output = tc1
        .simulate_receive_move_token(friend_move_token.clone())
        .unwrap();

    let move_token_received = match receive_move_token_output {
        ReceiveMoveTokenOutput::Received(move_token_received) => move_token_received,
        _ => unreachable!(),
    };

    assert!(move_token_received.currencies[0]
        .incoming_messages
        .is_empty());
    assert_eq!(move_token_received.mutations.len(), 2);

    let mut seen_mc_mutation = false;
    let mut seen_set_direction = false;

    for i in 0..2 {
        match &move_token_received.mutations[i] {
            TcMutation::McMutation(mc_mutation) => {
                seen_mc_mutation = true;
                assert_eq!(
                    mc_mutation,
                    &(currency.clone(), McMutation::SetLocalMaxDebt(100))
                );
            }
            TcMutation::SetDirection(set_direction) => {
                seen_set_direction = true;
                match set_direction {
                    SetDirection::Incoming(incoming_friend_move_token) => assert_eq!(
                        &create_hashed(&friend_move_token, &token_info),
                        incoming_friend_move_token
                    ),
                    _ => unreachable!(),
                }
            }
            _ => unreachable!(),
        }
    }
    assert!(seen_mc_mutation && seen_set_direction);

    for tc_mutation in &move_token_received.mutations {
        tc1.mutate(tc_mutation);
    }

    assert!(!tc1.get_outgoing().is_some());
    match &tc1.direction {
        TcDirection::Outgoing(_) => unreachable!(),
        TcDirection::Incoming(tc_incoming) => {
            assert_eq!(
                tc_incoming.move_token_in,
                create_hashed(&friend_move_token, &token_info)
            );
        }
    };

    assert_eq!(
        tc1.mutual_credits
            .get(&currency)
            .unwrap()
            .state()
            .balance
            .local_max_debt,
        100
    );
}
*/

/// This tests sends a SetRemoteMaxDebt(100) in both ways.
#[test]
fn test_simulate_receive_move_token_basic() {
    let currency = Currency::try_from("FST".to_owned()).unwrap();

    let mut rng1 = DummyRandom::new(&[1u8]);
    let pkcs8 = PrivateKey::rand_gen(&mut rng1);
    let identity1 = SoftwareEd25519Identity::from_private_key(&pkcs8).unwrap();

    let mut rng2 = DummyRandom::new(&[2u8]);
    let pkcs8 = PrivateKey::rand_gen(&mut rng2);
    let identity2 = SoftwareEd25519Identity::from_private_key(&pkcs8).unwrap();

    let (identity1, identity2) = sort_sides(identity1, identity2);

    let pk1 = identity1.get_public_key();
    let pk2 = identity2.get_public_key();
    let mut tc1 = TokenChannel::<u32>::new(&pk1, &pk2); // (local, remote)
    let mut tc2 = TokenChannel::<u32>::new(&pk2, &pk1); // (local, remote)

    // Current state:  tc1 --> tc2
    // tc1: outgoing
    // tc2: incoming
    add_currencies(
        &identity1,
        &identity2,
        &mut tc1,
        &mut tc2,
        &[currency.clone()],
    );

    // Current state:  tc2 --> tc1
    // tc1: incoming
    // tc2: outgoing
    add_currencies(
        &identity2,
        &identity1,
        &mut tc2,
        &mut tc1,
        &[currency.clone()],
    );

    // Current state:  tc1 --> tc2
    // tc1: outgoing
    // tc2: incoming
    // set_remote_max_debt21(&identity1, &identity2, &mut tc1, &mut tc2, &currency);

    // Current state:  tc2 --> tc1
    // tc1: incoming
    // tc2: outgoing
    // set_remote_max_debt21(&identity2, &identity1, &mut tc2, &mut tc1, &currency);
}

// TODO: Add more tests.
// - Test behaviour of Duplicate, ChainInconsistency
