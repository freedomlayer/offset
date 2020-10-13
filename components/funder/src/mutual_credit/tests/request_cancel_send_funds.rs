#[test]
fn test_request_cancel_send_funds() {
    let currency = Currency::try_from("FST".to_owned()).unwrap();

    let mut rng = DummyRandom::new(&[1u8]);
    let private_key = PrivateKey::rand_gen(&mut rng);
    let identity = SoftwareEd25519Identity::from_private_key(&private_key).unwrap();
    let public_key_b = identity.get_public_key();

    let local_public_key = PublicKey::from(&[0xaa; PublicKey::len()]);
    let remote_public_key = public_key_b.clone();
    let balance = 0;
    let mut mutual_credit =
        MutualCredit::new(&local_public_key, &remote_public_key, &currency, balance);

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

    apply_outgoing(
        &mut mutual_credit,
        &FriendTcOp::RequestSendFunds(request_send_funds),
    )
    .unwrap();

    assert_eq!(mutual_credit.state().balance.balance, 0);
    assert_eq!(mutual_credit.state().balance.local_pending_debt, 10 + 5);
    assert_eq!(mutual_credit.state().balance.remote_pending_debt, 0);

    // -----[CancelSendFunds]--------
    // ------------------------------
    let cancel_send_funds = CancelSendFundsOp { request_id };

    apply_incoming(
        &mut mutual_credit,
        FriendTcOp::CancelSendFunds(cancel_send_funds),
        100,
    )
    .unwrap();

    assert_eq!(mutual_credit.state().balance.balance, 0);
    assert_eq!(mutual_credit.state().balance.local_pending_debt, 0);
    assert_eq!(mutual_credit.state().balance.remote_pending_debt, 0);
}
