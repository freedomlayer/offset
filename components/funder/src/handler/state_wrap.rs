use std::fmt::Debug;

use common::canonical_serialize::CanonicalSerialize;

use crypto::crypto_rand::{CryptoRandom, RandValue};
use crypto::hash_lock::PlainLock;
use crypto::identity::PublicKey;

use proto::funder::messages::PendingTransaction;

use identity::IdentityClient;

use crate::state::{FunderMutation, FunderState};

use crate::ephemeral::{Ephemeral, EphemeralMutation};
use crate::friend::{BackwardsOp, FriendMutation};
use crate::types::create_response_send_funds;

#[derive(Debug, Clone)]
pub struct SemiResponse {
    friend_public_key: PublicKey,
    pending_transaction: PendingTransaction,
}

pub struct MutableFunderState<B: Clone> {
    initial_state: FunderState<B>,
    state: FunderState<B>,
    unsigned_responses: Vec<SemiResponse>,
    mutations: Vec<FunderMutation<B>>,
}

impl<B> MutableFunderState<B>
where
    B: Clone + CanonicalSerialize + PartialEq + Eq + Debug,
{
    pub fn new(state: FunderState<B>) -> Self {
        MutableFunderState {
            initial_state: state.clone(),
            state,
            unsigned_responses: Vec::new(),
            mutations: Vec::new(),
        }
    }

    /// Push an unsigned response operation.
    /// We have a separate queue for these operations because we need an async function call to
    /// sign an unsigned response.
    pub fn queue_unsigned_response(
        &mut self,
        friend_public_key: PublicKey,
        pending_transaction: PendingTransaction,
    ) {
        self.unsigned_responses.push(SemiResponse {
            friend_public_key,
            pending_transaction,
        });
    }

    pub fn mutate(&mut self, mutation: FunderMutation<B>) {
        self.state.mutate(&mutation);
        self.mutations.push(mutation);
    }

    pub fn state(&self) -> &FunderState<B> {
        &self.state
    }

    /// Sign all unsigned responses and apply them as mutations
    pub async fn sign_responses<'a, R>(
        &'a mut self,
        identity_client: &'a mut IdentityClient,
        rng: &'a R,
    ) where
        R: CryptoRandom,
    {
        while let Some(semi_response) = self.unsigned_responses.pop() {
            let SemiResponse {
                friend_public_key,
                pending_transaction,
            } = semi_response;

            // Randomly generate a dest plain lock:
            let dest_plain_lock = PlainLock::new(rng);

            // Mutation to push the new response:
            let rand_nonce = RandValue::new(rng);
            let response_send_funds = await!(create_response_send_funds(
                &pending_transaction,
                dest_plain_lock.hash(),
                rand_nonce,
                identity_client,
            ));

            let backwards_op = BackwardsOp::Response(response_send_funds);
            let friend_mutation = FriendMutation::PushBackPendingBackwardsOp(backwards_op);
            let funder_mutation =
                FunderMutation::FriendMutation((friend_public_key.clone(), friend_mutation));
            self.mutate(funder_mutation);

            // Mutation to add the destination plain lock:
            let funder_mutation = FunderMutation::AddIncomingTransaction((
                pending_transaction.invoice_id,
                pending_transaction.request_id,
                dest_plain_lock,
            ));
            self.mutate(funder_mutation);
        }
    }

    pub fn done(self) -> (FunderState<B>, Vec<FunderMutation<B>>, FunderState<B>) {
        // TODO: Find out how to change this into compile time guarantee:
        assert!(self.unsigned_responses.is_empty());
        (self.initial_state, self.mutations, self.state)
    }
}

/*
pub struct MutableFunderStateSigned<B: Clone> {
    initial_state: FunderState<B>,
    state: FunderState<B>,
    mutations: Vec<FunderMutation<B>>,
}

impl<B> MutableFunderStateSigned<B> where B: Clone + CanonicalSerialize + PartialEq + Eq + Debug {}
*/

pub struct MutableEphemeral {
    ephemeral: Ephemeral,
    mutations: Vec<EphemeralMutation>,
}

impl MutableEphemeral {
    pub fn new(ephemeral: Ephemeral) -> Self {
        MutableEphemeral {
            ephemeral,
            mutations: Vec::new(),
        }
    }
    pub fn mutate(&mut self, mutation: EphemeralMutation) {
        self.ephemeral.mutate(&mutation);
        self.mutations.push(mutation);
    }

    pub fn ephemeral(&self) -> &Ephemeral {
        &self.ephemeral
    }

    pub fn done(self) -> (Vec<EphemeralMutation>, Ephemeral) {
        (self.mutations, self.ephemeral)
    }
}
