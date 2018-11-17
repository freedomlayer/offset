#![warn(unused)]

use std::convert::TryFrom;

use crypto::identity::{PublicKey, Signature, PUBLIC_KEY_LEN, SIGNATURE_LEN};
use crypto::crypto_rand::{RandValue, RAND_VALUE_LEN};
use crypto::hash::sha_512_256;
use identity::IdentityClient;

use crate::consts::MAX_OPERATIONS_IN_BATCH;

use crate::mutual_credit::types::{MutualCredit, McMutation};
use crate::mutual_credit::incoming::{ProcessOperationOutput, ProcessTransListError, 
    process_operations_list, IncomingMessage};
use crate::mutual_credit::outgoing::OutgoingMc;

use crate::types::{FriendMoveToken, FriendMoveTokenHashed,
    FriendMoveTokenRequest, FriendTcOp};


#[derive(Debug)]
pub enum SetDirection {
    Incoming(FriendMoveTokenHashed), 
    Outgoing(FriendMoveToken),
}

#[allow(unused)]
#[derive(Debug)]
pub enum TcMutation {
    McMutation(McMutation),
    SetDirection(SetDirection),
    SetTokenWanted,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct TcOutgoing {
    pub mutual_credit: MutualCredit,
    pub move_token_out: FriendMoveToken,
    pub token_wanted: bool,
    pub opt_prev_move_token_in: Option<FriendMoveTokenHashed>,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct TcIncoming {
    pub mutual_credit: MutualCredit,
    pub move_token_in: FriendMoveTokenHashed,
}

#[derive(Clone, Serialize, Deserialize)]
pub enum TcDirection {
    Incoming(TcIncoming),
    Outgoing(TcOutgoing),
}


#[derive(Clone, Serialize, Deserialize)]
pub struct TokenChannel {
    direction: TcDirection,
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
}

#[derive(Debug)]
pub struct MoveTokenReceived {
    pub incoming_messages: Vec<IncomingMessage>,
    pub mutations: Vec<TcMutation>,
    pub remote_requests_closed: bool,
}


#[derive(Debug)]
pub enum ReceiveMoveTokenOutput {
    Duplicate,
    RetransmitOutgoing(FriendMoveToken),
    Received(MoveTokenReceived),
    // In case of a reset, all the local pending requests will be canceled.
}


/// Create a token from a public key
/// Currently this function puts the public key in the beginning of the signature buffer,
/// as the public key is shorter than a signature.
/// Possibly change this in the future (Maybe use a hash to spread the public key over all the
/// bytes of the signature)
///
/// Note that the output here is not a real signature. This function is used for the first
/// deterministic initialization of a token channel.
fn token_from_public_key(public_key: &PublicKey) -> Signature {
    let mut buff = [0; SIGNATURE_LEN];
    buff[0 .. PUBLIC_KEY_LEN].copy_from_slice(public_key);
    Signature::from(buff)
}

/// Generate a random nonce from public key.
/// Note that the result here is not really a random nonce. This function is used for the first
/// deterministic initialization of a token channel.
fn rand_nonce_from_public_key(public_key: &PublicKey) -> RandValue {
    let public_key_hash = sha_512_256(public_key);
    RandValue::try_from(&public_key_hash.as_ref()[.. RAND_VALUE_LEN]).unwrap()
}

/// Create an initial move token in the relationship between two public keys.
/// To canonicalize the initial move token (Having an equal move token for both sides), we sort the
/// two public keys in some way.
fn initial_move_token(low_public_key: &PublicKey, high_public_key: &PublicKey) -> FriendMoveToken {
    // This is a special initialization case.
    // Note that this is the only case where new_token is not a valid signature.
    // We do this because we want to have synchronization between the two sides of the token
    // channel, however, the remote side has no means of generating the signature (Because he
    // doesn't have the private key). Therefore we use a dummy new_token instead.
    let rand_nonce = rand_nonce_from_public_key(&high_public_key);
    FriendMoveToken {
        operations: Vec::new(),
        old_token: token_from_public_key(&low_public_key),
        inconsistency_counter: 0, 
        move_token_counter: 0,
        balance: 0,
        local_pending_debt: 0,
        remote_pending_debt: 0,
        rand_nonce,
        new_token: token_from_public_key(&high_public_key),
    }
}

/// Check if one public key is "lower" than another.
/// This is used to decide which side begins the token channel.
pub fn is_public_key_lower(pk1: &PublicKey, pk2: &PublicKey) -> bool {
    sha_512_256(pk1) < sha_512_256(pk2)
}

impl TokenChannel {
    pub fn new(local_public_key: &PublicKey, 
               remote_public_key: &PublicKey,
               balance: i128) -> TokenChannel {

        let mutual_credit = MutualCredit::new(&local_public_key, &remote_public_key, balance);

        if is_public_key_lower(&local_public_key, &remote_public_key) {
            // We are the first sender
            let tc_outgoing = TcOutgoing {
                mutual_credit,
                move_token_out: initial_move_token(local_public_key, remote_public_key),
                token_wanted: false,
                opt_prev_move_token_in: None,
            };
            TokenChannel {
                direction: TcDirection::Outgoing(tc_outgoing),
            }
        } else {
            // We are the second sender
            let tc_incoming = TcIncoming {
                mutual_credit,
                move_token_in: initial_move_token(remote_public_key, local_public_key)
                    .create_hashed(),
            };
            TokenChannel {
                direction: TcDirection::Incoming(tc_incoming),
            }
        }
    }

    pub fn new_from_remote_reset(local_public_key: &PublicKey, 
                      remote_public_key: &PublicKey, 
                      reset_move_token: &FriendMoveToken,
                      balance: i128) -> TokenChannel { // is balance redundant here?

        let tc_incoming = TcIncoming {
            mutual_credit: MutualCredit::new(local_public_key, remote_public_key, balance),
            move_token_in: reset_move_token.create_hashed(),
        };

        TokenChannel {
            direction: TcDirection::Incoming(tc_incoming),
        }
    }

    pub fn new_from_local_reset(local_public_key: &PublicKey, 
                      remote_public_key: &PublicKey, 
                      reset_move_token: &FriendMoveToken,
                      balance: i128, // Is this redundant?
                      opt_last_incoming_move_token: Option<FriendMoveTokenHashed>) -> TokenChannel {

        let tc_outgoing = TcOutgoing {
            mutual_credit: MutualCredit::new(local_public_key, remote_public_key, balance),
            move_token_out: reset_move_token.clone(),
            token_wanted: false,
            opt_prev_move_token_in: opt_last_incoming_move_token,
        };
        TokenChannel {
            direction: TcDirection::Outgoing(tc_outgoing),
        }
    }

    /// Get a reference to internal mutual_credit.
    pub fn get_mutual_credit(&self) -> &MutualCredit {
        match &self.direction {
            TcDirection::Incoming(tc_incoming) => &tc_incoming.mutual_credit,
            TcDirection::Outgoing(tc_outgoing) => &tc_outgoing.mutual_credit,
        }
    }

    pub fn get_remote_max_debt(&self) -> u128 {
        self.get_mutual_credit().state().balance.remote_max_debt
    }

    pub fn get_direction(&self) -> &TcDirection {
        &self.direction
    }

    /// Get the last incoming move token
    /// If no such incoming move token exists (Maybe this is the beginning of the relationship),
    /// returns None.
    pub fn get_last_incoming_move_token_hashed(&self) -> Option<&FriendMoveTokenHashed> {
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

    pub fn mutate(&mut self, d_mutation: &TcMutation) {
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
                            token_wanted: false,
                            opt_prev_move_token_in: self.get_last_incoming_move_token_hashed().cloned(),
                        };
                        TcDirection::Outgoing(tc_outgoing)
                    }
                };
            },
            TcMutation::SetTokenWanted => {
                match self.direction {
                    TcDirection::Incoming(_) => unreachable!(),
                    TcDirection::Outgoing(ref mut tc_outgoing) => {
                        tc_outgoing.token_wanted = true;
                    },
                }
            },
        }
    }

    fn get_cur_move_token_hashed(&self) -> FriendMoveTokenHashed {
        match &self.direction {
            TcDirection::Incoming(tc_incoming) => tc_incoming.move_token_in.clone(),
            TcDirection::Outgoing(tc_outgoing) => tc_outgoing.move_token_out.create_hashed(),
        }
    }


    pub fn get_inconsistency_counter(&self) -> u64 {
        self.get_cur_move_token_hashed().inconsistency_counter
    }

    pub fn get_move_token_counter(&self) -> u128 {
        self.get_cur_move_token_hashed().move_token_counter
    }

    /// Get the current new token (Either incoming or outgoing)
    /// This is the most recent token in the chain.
    pub fn get_new_token(&self) -> &Signature {
        match &self.direction {
            TcDirection::Incoming(tc_incoming) => &tc_incoming.move_token_in.new_token,
            TcDirection::Outgoing(tc_outgoing) => &tc_outgoing.move_token_out.new_token,
        }
    }

    pub fn is_outgoing(&self) -> bool {
        match self.direction {
            TcDirection::Incoming(_) => false,
            TcDirection::Outgoing(_) => true,
        }
    }

    pub fn simulate_receive_move_token(&self, 
                              new_move_token: FriendMoveToken)
        -> Result<ReceiveMoveTokenOutput, ReceiveMoveTokenError> {

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


impl TcIncoming {
    /// Handle an incoming move token during Incoming direction:
    fn handle_incoming(&self, 
                        new_move_token: FriendMoveToken) 
        -> Result<ReceiveMoveTokenOutput, ReceiveMoveTokenError> {
        // We compare the whole move token message and not just the signature (new_token)
        // because we don't check the signature in this flow.
        if &self.move_token_in == &new_move_token.create_hashed() {
            // Duplicate
            Ok(ReceiveMoveTokenOutput::Duplicate)
        } else {
            // Inconsistency
            Err(ReceiveMoveTokenError::ChainInconsistency)
        }
    }

    pub async fn create_friend_move_token(&self,
                                    operations: Vec<FriendTcOp>,
                                    rand_nonce: RandValue,
                                    identity_client: IdentityClient) -> FriendMoveToken {
        // TODO: How to make this check happen only in debug?
        let identity_pk = await!(identity_client.request_public_key()).unwrap();
        assert_eq!(&identity_pk, &self.mutual_credit.state().idents.local_public_key);

        await!(FriendMoveToken::new(
            operations,
            self.move_token_in.new_token.clone(),
            self.move_token_in.inconsistency_counter,
            self.move_token_in.move_token_counter.wrapping_add(1),
            self.mutual_credit.state().balance.balance,
            self.mutual_credit.state().balance.local_pending_debt,
            self.mutual_credit.state().balance.remote_pending_debt,
            rand_nonce,
            identity_client))
    }

    pub fn begin_outgoing_move_token(&self) -> OutgoingMc {
        // TODO; Maybe take max_operations_in_batch as argument instead?
        OutgoingMc::new(&self.mutual_credit, MAX_OPERATIONS_IN_BATCH)
    }
}



impl TcOutgoing {
    /// Handle an incoming move token during Outgoing direction:
    fn handle_incoming(&self, 
                        new_move_token: FriendMoveToken) 
        -> Result<ReceiveMoveTokenOutput, ReceiveMoveTokenError> {

        // Verify signature:
        // Note that we only verify the signature here, and not at the Incoming part.
        // This allows the genesis move token to occur smoothly, even though its signature
        // is not correct.
        let remote_public_key = &self.mutual_credit.state().idents.remote_public_key;
        if !new_move_token.verify(remote_public_key) {
            return Err(ReceiveMoveTokenError::InvalidSignature);
        }

        // let friend_move_token = &tc_outgoing.move_token_out;
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
                                   new_move_token: FriendMoveToken)
        -> Result<ReceiveMoveTokenOutput, ReceiveMoveTokenError> {
    
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
                    TcMutation::SetDirection(SetDirection::Incoming(new_move_token.create_hashed())));

                let move_token_received = MoveTokenReceived {
                    incoming_messages,
                    mutations,
                    // Were the remote requests initially open and now it is closed?
                    remote_requests_closed: final_remote_requests && !initial_remote_requests,
                };

                Ok(ReceiveMoveTokenOutput::Received(move_token_received))
            },
            Err(e) => {
                Err(ReceiveMoveTokenError::InvalidTransaction(e))
            },
        }
    }



    /// Get the current outgoing move token
    pub fn create_outgoing_move_token_request(&self) -> FriendMoveTokenRequest {
        FriendMoveTokenRequest {
            friend_move_token: self.move_token_out.clone(),
            token_wanted: self.token_wanted,
        }
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
        let token_channel_a_b = TokenChannel::new(&pk_a, &pk_b, 0i128);
        let token_channel_b_a = TokenChannel::new(&pk_b, &pk_a, 0i128);

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

    #[test]
    fn test_set_token_wanted() {
        let pk_a = PublicKey::from(&[0xaa; PUBLIC_KEY_LEN]);
        let pk_b = PublicKey::from(&[0xbb; PUBLIC_KEY_LEN]);
        let token_channel_a_b = TokenChannel::new(&pk_a, &pk_b, 0i128);
        let token_channel_b_a = TokenChannel::new(&pk_b, &pk_a, 0i128);

        // Only one of those token channels is outgoing:
        let is_a_b_outgoing = token_channel_a_b.is_outgoing();
        let is_b_a_outgoing = token_channel_b_a.is_outgoing();
        assert!(is_a_b_outgoing ^ is_b_a_outgoing);

        let (mut out_tc, _in_tc) = if is_a_b_outgoing {
            (token_channel_a_b, token_channel_b_a)
        } else {
            (token_channel_b_a, token_channel_a_b)
        };

        // First token is in not wanted state:
        let tc_outgoing = match out_tc.get_direction() {
            TcDirection::Outgoing(tc_outgoing) => tc_outgoing,
            TcDirection::Incoming(_) => unreachable!(),
        };
        assert!(!tc_outgoing.token_wanted);

        let tc_mutation = TcMutation::SetTokenWanted;
        out_tc.mutate(&tc_mutation);

        // Then token is in wanted state:
        let tc_outgoing = match out_tc.get_direction() {
            TcDirection::Outgoing(tc_outgoing) => tc_outgoing,
            TcDirection::Incoming(_) => unreachable!(),
        };
        assert!(tc_outgoing.token_wanted);
    }

    /// Sort the two identity client.
    /// The result will be a pair where the first is initially configured to have outgoing message,
    /// and the second is initially configured to have incoming message.
    async fn sort_sides(identity_client1: IdentityClient,
                        identity_client2: IdentityClient) 
        -> (IdentityClient, IdentityClient) {

        let pk1 = await!(identity_client1.request_public_key()).unwrap();
        let pk2 = await!(identity_client2.request_public_key()).unwrap();
        let token_channel12 = TokenChannel::new(&pk1, &pk2, 0i128); // (local, remote)
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
                            tc1: &'a mut TokenChannel,
                            tc2: &'a mut TokenChannel) {

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
        let friend_move_token = await!(tc2_incoming.create_friend_move_token(operations, 
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
                            assert_eq!(&friend_move_token.create_hashed(), incoming_friend_move_token),
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
        assert_eq!(&tc1.get_cur_move_token_hashed(), &friend_move_token.create_hashed());
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
