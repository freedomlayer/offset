use std::fmt::Debug;

use signature::canonical::CanonicalSerialize;

use crypto::hash_lock::HashLock;
use crypto::rand::{CryptoRandom, RandGen};

use proto::crypto::{PublicKey, RandValue};
use proto::funder::messages::{Currency, PendingTransaction};

use identity::IdentityClient;

use crate::state::{FunderMutation, FunderState};

use crate::ephemeral::{Ephemeral, EphemeralMutation};
use crate::friend::{BackwardsOp, FriendMutation};
use crate::types::create_response_send_funds;

#[derive(Debug, Clone)]
pub struct SemiResponse {
    friend_public_key: PublicKey,
    currency: Currency,
    pending_transaction: PendingTransaction,
    is_complete: bool,
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
        currency: Currency,
        pending_transaction: PendingTransaction,
        is_complete: bool,
    ) {
        self.unsigned_responses.push(SemiResponse {
            friend_public_key,
            currency,
            pending_transaction,
            is_complete,
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
                currency,
                pending_transaction,
                is_complete,
            } = semi_response;

            // Get corresponding dest_plain_lock:
            let dest_plain_lock =
                self.state().open_invoices.get(&pending_transaction.invoice_id).unwrap().dest_plain_lock.clone();

            // Mutation to push the new response:
            let rand_nonce = RandValue::rand_gen(rng);

            let response_send_funds = create_response_send_funds(
                &currency,
                &pending_transaction,
                dest_plain_lock.hash_lock(),
                is_complete,
                rand_nonce,
                identity_client,
            )
            .await;

            let backwards_op = BackwardsOp::Response(response_send_funds);
            let friend_mutation =
                FriendMutation::PushBackPendingBackwardsOp((currency, backwards_op));
            let funder_mutation =
                FunderMutation::FriendMutation((friend_public_key.clone(), friend_mutation));
            self.mutate(funder_mutation);

        }
    }

    pub fn done(self) -> (FunderState<B>, Vec<FunderMutation<B>>, FunderState<B>) {
        // TODO: Find out how to change this into compile time guarantee:
        assert!(self.unsigned_responses.is_empty());
        (self.initial_state, self.mutations, self.state)
    }
}

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
