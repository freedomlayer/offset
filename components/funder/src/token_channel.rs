#![warn(unused)]

use std::cmp::Ordering;
use std::convert::TryFrom;

use common::canonical_serialize::CanonicalSerialize;

use crypto::identity::{PublicKey, Signature, PUBLIC_KEY_LEN, 
    SIGNATURE_LEN, compare_public_key};
use crypto::crypto_rand::{RandValue, RAND_VALUE_LEN};
use crypto::hash::sha_512_256;

use proto::funder::messages::{SignedMoveToken, FriendTcOp, TSignature, TPublicKey,
                            MoveToken};

use crate::mutual_credit::types::{MutualCredit, McMutation};
use crate::mutual_credit::incoming::{ProcessOperationOutput, ProcessTransListError, 
    process_operations_list, IncomingMessage};
use crate::mutual_credit::outgoing::OutgoingMc;
use crate::sign_verify::Verify;

use crate::types::{MoveTokenHashed, create_unsigned_move_token, create_hashed};


#[derive(Debug)]
pub enum SetDirection<A,P:Clone,RS,FS,MS> {
    Incoming(MoveTokenHashed<P,MS>), 
    Outgoing(SignedMoveToken<A,P,RS,FS,MS>),
}

#[allow(unused)]
#[derive(Debug)]
pub enum TcMutation<A,P:Clone,RS,FS,MS> {
    McMutation(McMutation<P>),
    SetDirection(SetDirection<A,P,RS,FS,MS>),
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct TcOutgoing<A,P:Clone,RS,FS,MS> {
    pub mutual_credit: MutualCredit<P>,
    pub move_token_out: SignedMoveToken<A,P,RS,FS,MS>,
    pub opt_prev_move_token_in: Option<MoveTokenHashed<P,MS>>,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct TcIncoming<P:Clone,MS> {
    pub mutual_credit: MutualCredit<P>,
    pub move_token_in: MoveTokenHashed<P,MS>,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub enum TcDirection<A,P:Clone,RS,FS,MS> {
    Incoming(TcIncoming<P,MS>),
    Outgoing(TcOutgoing<A,P,RS,FS,MS>),
}


#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct TokenChannel<A,P:Clone,RS,FS,MS> {
    direction: TcDirection<A,P,RS,FS,MS>,
}

#[derive(Debug)]
pub enum ReceiveMoveTokenError {
    ChainInconsistency,
    InvalidTransaction(ProcessTransListError),
    InvalidSignature,
    InvalidStatedBalance,
    InvalidInconsistencyCounter,
    MoveTokenCounterOverflow,
    InvalidMoveTokenCounter,
    TooManyOperations,
}

#[derive(Debug)]
pub struct MoveTokenReceived<A,P:Clone,RS,FS,MS> {
    pub incoming_messages: Vec<IncomingMessage<P,RS,FS>>,
    pub mutations: Vec<TcMutation<A,P,RS,FS,MS>>,
    pub remote_requests_closed: bool,
    pub opt_local_address: Option<A>,
}


#[derive(Debug)]
pub enum ReceiveMoveTokenOutput<A,P:Clone,RS,FS,MS> {
    Duplicate,
    RetransmitOutgoing(SignedMoveToken<A,P,RS,FS,MS>),
    Received(MoveTokenReceived<A,P,RS,FS,MS>),
    // In case of a reset, all the local pending requests will be canceled.
}

impl<A,P,RS,FS,MS> TokenChannel<A,P,RS,FS,MS> 
where
    A: CanonicalSerialize + Clone,
    P: Clone,
{
    pub fn new(local_public_key: &TPublicKey<P>, 
               remote_public_key: &TPublicKey<P>,
               balance: i128) -> Self {

        let mutual_credit = MutualCredit::new(&local_public_key, &remote_public_key, balance);

        if compare_public_key(&local_public_key, &remote_public_key) == Ordering::Less {
            // We are the first sender
            let tc_outgoing = TcOutgoing {
                mutual_credit,
                move_token_out: MoveToken::initial(local_public_key, 
                                                   remote_public_key, 
                                                   balance),
                opt_prev_move_token_in: None,
            };
            TokenChannel {
                direction: TcDirection::Outgoing(tc_outgoing),
            }
        } else {
            // We are the second sender
            let tc_incoming = TcIncoming {
                mutual_credit,
                move_token_in: create_hashed::<A>(&MoveToken::initial(remote_public_key, 
                                                                local_public_key, 
                                                                balance.checked_neg().unwrap())),
            };
            TokenChannel {
                direction: TcDirection::Incoming(tc_incoming),
            }
        }
    }

    pub fn new_from_remote_reset(local_public_key: &TPublicKey<P>, 
                      remote_public_key: &TPublicKey<P>, 
                      reset_move_token: &SignedMoveToken<A,P,RS,FS,MS>,
                      balance: i128) -> Self { // is balance redundant here?

        let tc_incoming = TcIncoming {
            mutual_credit: MutualCredit::new(local_public_key, remote_public_key, balance),
            move_token_in: create_hashed(&reset_move_token),
        };

        TokenChannel {
            direction: TcDirection::Incoming(tc_incoming),
        }
    }

    pub fn new_from_local_reset(local_public_key: &TPublicKey<P>, 
                      remote_public_key: &TPublicKey<P>, 
                      reset_move_token: &SignedMoveToken<A,P,RS,FS,MS>,
                      balance: i128, // Is this redundant?
                      opt_last_incoming_move_token: Option<MoveTokenHashed>) -> Self {

        let tc_outgoing = TcOutgoing {
            mutual_credit: MutualCredit::new(local_public_key, remote_public_key, balance),
            move_token_out: reset_move_token.clone(),
            opt_prev_move_token_in: opt_last_incoming_move_token,
        };
        TokenChannel {
            direction: TcDirection::Outgoing(tc_outgoing),
        }
    }

    /// Get a reference to internal mutual_credit.
    pub fn get_mutual_credit(&self) -> &MutualCredit<P> {
        match &self.direction {
            TcDirection::Incoming(tc_incoming) => &tc_incoming.mutual_credit,
            TcDirection::Outgoing(tc_outgoing) => &tc_outgoing.mutual_credit,
        }
    }

    pub fn get_remote_max_debt(&self) -> u128 {
        self.get_mutual_credit().state().balance.remote_max_debt
    }

    pub fn get_direction(&self) -> &TcDirection<A,P,RS,FS,MS> {
        &self.direction
    }

    /// Get the last incoming move token
    /// If no such incoming move token exists (Maybe this is the beginning of the relationship),
    /// returns None.
    pub fn get_last_incoming_move_token_hashed(&self) -> Option<&MoveTokenHashed<P,MS>> {
        match &self.direction {
            TcDirection::Incoming(tc_incoming) => Some(&tc_incoming.move_token_in),
            TcDirection::Outgoing(tc_outgoing) => {
                match &tc_outgoing.opt_prev_move_token_in {
                    None => None,
                    Some(prev_move_token_in) => Some(prev_move_token_in),
                }
            },
        }
    }

    pub fn mutate(&mut self, d_mutation: &TcMutation<A,P,RS,FS,MS>) {
        match d_mutation {
            TcMutation::McMutation(mc_mutation) => {
                let mutual_credit = match &mut self.direction {
                    TcDirection::Incoming(tc_incoming) => &mut tc_incoming.mutual_credit,
                    TcDirection::Outgoing(tc_outgoing) => &mut tc_outgoing.mutual_credit,
                };
                mutual_credit.mutate(mc_mutation);
            },
            TcMutation::SetDirection(ref set_direction) => {
                self.direction = match set_direction {
                    SetDirection::Incoming(friend_move_token_hashed) => {
                        let tc_incoming = TcIncoming {
                            mutual_credit: self.get_mutual_credit().clone(), // TODO: Remove this clone()
                            move_token_in: friend_move_token_hashed.clone(),
                        };
                        TcDirection::Incoming(tc_incoming)
                    },
                    SetDirection::Outgoing(friend_move_token) => {
                        let tc_outgoing = TcOutgoing {
                            mutual_credit: self.get_mutual_credit().clone(), // TODO; Remove this clone()
                            move_token_out: friend_move_token.clone(),
                            opt_prev_move_token_in: self.get_last_incoming_move_token_hashed().cloned(),
                        };
                        TcDirection::Outgoing(tc_outgoing)
                    }
                };
            },
        }
    }


    pub fn get_inconsistency_counter(&self) -> u64 {
        match &self.direction {
            TcDirection::Incoming(tc_incoming) => tc_incoming.move_token_in.inconsistency_counter,
            TcDirection::Outgoing(tc_outgoing) => tc_outgoing.move_token_out.inconsistency_counter,
        }
    }

    pub fn get_move_token_counter(&self) -> u128 {
        match &self.direction {
            TcDirection::Incoming(tc_incoming) => tc_incoming.move_token_in.move_token_counter,
            TcDirection::Outgoing(tc_outgoing) => tc_outgoing.move_token_out.move_token_counter,
        }
    }

    pub fn is_outgoing(&self) -> bool {
        match self.direction {
            TcDirection::Incoming(_) => false,
            TcDirection::Outgoing(_) => true,
        }
    }

    pub fn simulate_receive_move_token(&self, 
                              new_move_token: SignedMoveToken<A,P,RS,FS,MS>)
        -> Result<ReceiveMoveTokenOutput<A,P,RS,FS,MS>, ReceiveMoveTokenError> {

        match &self.direction {
            TcDirection::Incoming(tc_incoming) => {
                tc_incoming.handle_incoming(new_move_token)
            },
            TcDirection::Outgoing(tc_outgoing) => {
                tc_outgoing.handle_incoming(new_move_token)
            },
        }
    }
}


impl<P,MS> TcIncoming<P,MS> 
where
    P: Clone + Eq,
{
    /// Handle an incoming move token during Incoming direction:
    fn handle_incoming<A,RS>(&self, 
                        new_move_token: MoveToken<A,P,RS,MS>) 
        -> Result<ReceiveMoveTokenOutput<A,P,RS,MS>, ReceiveMoveTokenError> 
    where
        A: CanonicalSerialize,
    {

        // We compare the whole move token message and not just the signature (new_token)
        // because we don't check the signature in this flow.
        if &self.move_token_in == &create_hashed(&new_move_token) {
            // Duplicate
            Ok(ReceiveMoveTokenOutput::Duplicate)
        } else {
            // Inconsistency
            Err(ReceiveMoveTokenError::ChainInconsistency)
        }
    }

    pub fn create_unsigned_move_token<A,RS>(&self,
                                    operations: Vec<FriendTcOp<P,RS>>,
                                    opt_local_address: Option<A>,
                                    rand_nonce: RandValue) -> UnsignedMoveToken<A> {

        create_unsigned_move_token(
            operations,
            opt_local_address,
            self.move_token_in.new_token.clone(),
            self.move_token_in.inconsistency_counter,
            self.move_token_in.move_token_counter.wrapping_add(1),
            self.mutual_credit.state().balance.balance,
            self.mutual_credit.state().balance.local_pending_debt,
            self.mutual_credit.state().balance.remote_pending_debt,
            rand_nonce)
    }

    pub fn begin_outgoing_move_token(&self) -> OutgoingMc<P> {
        OutgoingMc::new(&self.mutual_credit)
    }
}



impl<A,P,RS,FS,MS> TcOutgoing<A,P,RS,FS,MS> 
where
    A: CanonicalSerialize + Clone,
    P: Clone,
    MS: Eq,
{
    /// Handle an incoming move token during Outgoing direction:
    fn handle_incoming(&self, 
                        new_move_token: SignedMoveToken<A,P,RS,FS,MS>) 
        -> Result<ReceiveMoveTokenOutput<A,P,RS,FS,MS>, ReceiveMoveTokenError> {

        if &new_move_token.old_token == &self.move_token_out.new_token {
            self.handle_incoming_token_match(new_move_token)
            // self.outgoing_to_incoming(friend_move_token, new_move_token)
        } else if self.move_token_out.old_token == new_move_token.new_token {
            // We should retransmit our move token message to the remote side.
            Ok(ReceiveMoveTokenOutput::RetransmitOutgoing(self.move_token_out.clone()))
        } else {
            Err(ReceiveMoveTokenError::ChainInconsistency)
        }
    }

    fn handle_incoming_token_match(&self,
                                   new_move_token: SignedMoveToken<A,P,RS,FS,MS>)
        -> Result<ReceiveMoveTokenOutput<A,P,RS,FS,MS>, ReceiveMoveTokenError> {

        // Verify signature:
        // Note that we only verify the signature here, and not at the Incoming part.
        // This allows the genesis move token to occur smoothly, even though its signature
        // is not correct.
        let remote_public_key = &self.mutual_credit.state().idents.remote_public_key;
        let pair = (&new_move_token, &remote_public_key);
        if !pair.verify {
            return Err(ReceiveMoveTokenError::InvalidSignature);
        }
    
        // Verify counters:
        if new_move_token.inconsistency_counter != self.move_token_out.inconsistency_counter {
            return Err(ReceiveMoveTokenError::InvalidInconsistencyCounter);
        }

        let expected_move_token_counter = self.move_token_out.move_token_counter
            .checked_add(1)
            .ok_or(ReceiveMoveTokenError::MoveTokenCounterOverflow)?;

        if new_move_token.move_token_counter != expected_move_token_counter {
            return Err(ReceiveMoveTokenError::InvalidMoveTokenCounter);
        }

        let mut mutual_credit = self.mutual_credit.clone();
        let res = process_operations_list(&mut mutual_credit,
            new_move_token.operations.clone());

        match res {
            Ok(outputs) => {
                let initial_remote_requests = self
                                                .mutual_credit
                                                .state()
                                                .requests_status
                                                .remote
                                                .is_open();

                let mut incoming_messages = Vec::new();
                let mut mutations =  Vec::new();

                // We apply mutations on this token channel, to verify stated balance values
                let mut check_mutual_credit = self.mutual_credit.clone();

                let mut final_remote_requests: bool = initial_remote_requests;
                for output in outputs {
                    let ProcessOperationOutput 
                        {incoming_message, mc_mutations} = output;

                    if let Some(funds) = incoming_message {
                        incoming_messages.push(funds);
                    }
                    for mc_mutation in mc_mutations {
                        check_mutual_credit.mutate(&mc_mutation);
                        if let McMutation::SetRemoteRequestsStatus(requests_status) = &mc_mutation {
                            final_remote_requests = requests_status.is_open();
                        }
                        mutations.push(
                            TcMutation::McMutation(mc_mutation));
                    }
                }

                // Verify stated balances:
                let check_balance = &check_mutual_credit.state().balance;
                if check_balance.balance != -new_move_token.balance ||
                   check_balance.local_pending_debt != new_move_token.remote_pending_debt ||
                   check_balance.remote_pending_debt != new_move_token.local_pending_debt {
                    return Err(ReceiveMoveTokenError::InvalidStatedBalance);
                }

                mutations.push(
                    TcMutation::SetDirection(SetDirection::Incoming(create_hashed(&new_move_token))));

                let move_token_received = MoveTokenReceived {
                    incoming_messages,
                    mutations,
                    // Were the remote requests initially open and now it is closed?
                    remote_requests_closed: final_remote_requests && !initial_remote_requests,
                    opt_local_address: new_move_token.opt_local_address.clone(),
                };

                Ok(ReceiveMoveTokenOutput::Received(move_token_received))
            },
            Err(e) => {
                Err(ReceiveMoveTokenError::InvalidTransaction(e))
            },
        }
    }

    /// Get the current outgoing move token
    pub fn create_outgoing_move_token(&self) -> SignedMoveToken<A,P,RS,FS,MS> {
        self.move_token_out.clone()
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    use futures::executor::ThreadPool;
    use futures::{future, FutureExt};
    use futures::task::SpawnExt;
    use identity::{create_identity, IdentityClient};

    use crypto::test_utils::DummyRandom;
    use crypto::identity::{SoftwareEd25519Identity,
                            generate_pkcs8_key_pair};

    #[test]
    fn test_initial_direction() {
        let pk_a = PublicKey::from(&[0xaa; PUBLIC_KEY_LEN]);
        let pk_b = PublicKey::from(&[0xbb; PUBLIC_KEY_LEN]);
        let token_channel_a_b = TokenChannel::<u32>::new(&pk_a, &pk_b, 0i128);
        let token_channel_b_a = TokenChannel::<u32>::new(&pk_b, &pk_a, 0i128);

        // Only one of those token channels is outgoing:
        let is_a_b_outgoing = token_channel_a_b.is_outgoing();
        let is_b_a_outgoing = token_channel_b_a.is_outgoing();
        assert!(is_a_b_outgoing ^ is_b_a_outgoing);

        let (out_tc, in_tc) = if is_a_b_outgoing {
            (token_channel_a_b, token_channel_b_a)
        } else {
            (token_channel_b_a, token_channel_a_b)
        };

        assert_eq!(out_tc.get_cur_move_token_hashed(), in_tc.get_cur_move_token_hashed());
        let tc_outgoing = match out_tc.get_direction() {
            TcDirection::Outgoing(tc_outgoing) => tc_outgoing,
            TcDirection::Incoming(_) => unreachable!(),
        };
        assert!(!tc_outgoing.token_wanted);
        assert!(tc_outgoing.opt_prev_move_token_in.is_none());
    }

    /// Sort the two identity client.
    /// The result will be a pair where the first is initially configured to have outgoing message,
    /// and the second is initially configured to have incoming message.
    async fn sort_sides(identity_client1: IdentityClient,
                        identity_client2: IdentityClient) 
        -> (IdentityClient, IdentityClient) {

        let pk1 = await!(identity_client1.request_public_key()).unwrap();
        let pk2 = await!(identity_client2.request_public_key()).unwrap();
        let token_channel12 = TokenChannel::<u32>::new(&pk1, &pk2, 0i128); // (local, remote)
        if token_channel12.is_outgoing() {
            (identity_client1, identity_client2)
        } else {
            (identity_client2, identity_client1)

        }
    }

    /// Before: tc1: outgoing, tc2: incoming
    /// Send SetRemoteMaxDebt: tc2 -> tc1
    /// After: tc1: incoming, tc2: outgoing
    async fn set_remote_max_debt21<'a>(_identity_client1: &'a mut IdentityClient,
                            identity_client2: &'a mut IdentityClient,
                            tc1: &'a mut TokenChannel<u32>,
                            tc2: &'a mut TokenChannel<u32>) {

        assert!(tc1.is_outgoing());
        assert!(!tc2.is_outgoing());

        let tc2_incoming = match tc2.get_direction() {
            TcDirection::Incoming(tc2_incoming) => tc2_incoming,
            TcDirection::Outgoing(_) => unreachable!(),
        };
        let mut outgoing_mc = tc2_incoming.begin_outgoing_move_token();
        let friend_tc_op = FriendTcOp::SetRemoteMaxDebt(100);
        outgoing_mc.queue_operation(friend_tc_op).unwrap();
        let (operations, mc_mutations) = outgoing_mc.done();


        let rand_nonce = RandValue::from(&[5; RAND_VALUE_LEN]);
        let opt_local_address = None;
        let friend_move_token = await!(tc2_incoming.create_friend_move_token(operations, 
                                                    opt_local_address,
                                                    rand_nonce, 
                                                    identity_client2.clone()));

        for mc_mutation in mc_mutations {
            tc2.mutate(&TcMutation::McMutation(mc_mutation));
        }
        let tc_mutation = TcMutation::SetDirection(
            SetDirection::Outgoing(friend_move_token.clone()));
        tc2.mutate(&tc_mutation);

        assert!(tc2.is_outgoing());

        let receive_move_token_output = 
            tc1.simulate_receive_move_token(friend_move_token.clone()).unwrap();

        let move_token_received = match receive_move_token_output {
            ReceiveMoveTokenOutput::Received(move_token_received) => move_token_received,
            _ => unreachable!(),
        };

        assert!(move_token_received.incoming_messages.is_empty());
        assert_eq!(move_token_received.mutations.len(), 2);

        let mut seen_mc_mutation = false;
        let mut seen_set_direction = false;

        for i in 0 .. 2 {
            match &move_token_received.mutations[i] {
                TcMutation::McMutation(mc_mutation) => {
                    seen_mc_mutation = true;
                    assert_eq!(mc_mutation, &McMutation::SetLocalMaxDebt(100));
                },
                TcMutation::SetDirection(set_direction) => {
                    seen_set_direction = true;
                    match set_direction {
                        SetDirection::Incoming(incoming_friend_move_token) =>
                            assert_eq!(&create_hashed(&friend_move_token), incoming_friend_move_token),
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

        assert!(!tc1.is_outgoing());
        assert_eq!(&tc1.get_cur_move_token_hashed(), &create_hashed(&friend_move_token));
        assert_eq!(tc1.get_mutual_credit().state().balance.local_max_debt, 100);

    }


    /// This tests sends a SetRemoteMaxDebt(100) in both ways.
    async fn task_simulate_receive_move_token_basic(identity_client1: IdentityClient, 
                                  identity_client2: IdentityClient) {

        let (mut identity_client1, mut identity_client2) = 
            await!(sort_sides(identity_client1, identity_client2));

        let pk1 = await!(identity_client1.request_public_key()).unwrap();
        let pk2 = await!(identity_client2.request_public_key()).unwrap();
        let mut tc1 = TokenChannel::new(&pk1, &pk2, 0i128); // (local, remote)
        let mut tc2 = TokenChannel::new(&pk2, &pk1, 0i128); // (local, remote)

        // Current state:  tc1 --> tc2
        // tc1: outgoing
        // tc2: incoming
        await!(set_remote_max_debt21(&mut identity_client1,
                              &mut identity_client2,
                              &mut tc1,
                              &mut tc2));

        // Current state:  tc2 --> tc1
        // tc1: incoming
        // tc2: outgoing
        await!(set_remote_max_debt21(&mut identity_client2,
                              &mut identity_client1,
                              &mut tc2,
                              &mut tc1));

    }

    #[test]
    fn test_simulate_receive_move_token_basic() {
        // Start identity service:
        let mut thread_pool = ThreadPool::new().unwrap();

        let rng1 = DummyRandom::new(&[1u8]);
        let pkcs8 = generate_pkcs8_key_pair(&rng1);
        let identity1 = SoftwareEd25519Identity::from_pkcs8(&pkcs8).unwrap();
        let (requests_sender1, identity_server1) = create_identity(identity1);
        let identity_client1 = IdentityClient::new(requests_sender1);

        let rng2 = DummyRandom::new(&[2u8]);
        let pkcs8 = generate_pkcs8_key_pair(&rng2);
        let identity2 = SoftwareEd25519Identity::from_pkcs8(&pkcs8).unwrap();
        let (requests_sender2, identity_server2) = create_identity(identity2);
        let identity_client2 = IdentityClient::new(requests_sender2);

        thread_pool.spawn(identity_server1.then(|_| future::ready(()))).unwrap();
        thread_pool.spawn(identity_server2.then(|_| future::ready(()))).unwrap();

        thread_pool.run(task_simulate_receive_move_token_basic(identity_client1, identity_client2));
    }


    // TODO: Add more tests.
    // - Test behaviour of Duplicate, ChainInconsistency
}
