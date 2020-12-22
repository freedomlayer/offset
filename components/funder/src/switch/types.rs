use common::async_rpc::AsyncOpResult;
use std::collections::HashMap;
// use common::ser_utils::ser_string;
// use common::u256::U256;

use crate::liveness::Liveness;
use crate::token_channel::TcDbClient;

use proto::app_server::messages::{NamedRelayAddress, RelayAddress};
use proto::crypto::PublicKey;
use proto::funder::messages::{
    CancelSendFundsOp, Currency, FriendMessage, RequestSendFundsOp, ResponseSendFundsOp,
};
use proto::index_server::messages::IndexMutation;
// use proto::funder::messages::{McBalance, PendingTransaction};
//

/// Switch's ephemeral state (Not saved inside the database)
#[derive(Debug)]
pub struct SwitchState {
    pub liveness: Liveness,
}

#[derive(Debug)]
pub enum BackwardsOp {
    Response(ResponseSendFundsOp),
    Cancel(CancelSendFundsOp),
}

pub trait SwitchDbClient {
    type TcDbClient: TcDbClient;
    fn tc_db_client(&mut self, friend_public_key: PublicKey) -> &mut Self::TcDbClient;

    /// Get the current list of local relays
    fn get_local_relays(&mut self) -> AsyncOpResult<Vec<NamedRelayAddress>>;

    /*
    /// Get the maximum value of sent relay generation.
    /// Returns None if all relays were acked by the remote side.
    fn get_max_sent_relays_generation(
        &mut self,
        friend_public_key: PublicKey,
    ) -> AsyncOpResult<Option<u128>>;
    */

    /// Get the last set of sent relays, together with their generation
    /// Generation == None means that the sent relays were already acked
    fn get_sent_relays(
        &mut self,
        friend_public_key: PublicKey,
    ) -> AsyncOpResult<(Option<u128>, Vec<RelayAddress>)>;

    /// Update sent relays for friend `friend_public_key` with the list of current local relays.
    fn update_sent_relays(
        &mut self,
        friend_public_key: PublicKey,
        generation: u128,
        sent_relays: Vec<RelayAddress>,
    ) -> AsyncOpResult<()>;

    /*
    fn get_balance(&mut self) -> AsyncOpResult<McBalance>;
    */

    /*
    fn peek_pending_backwards(
        &mut self,
        friend_public_key: PublicKey,
    ) -> AsyncOpResult<BackwardsOp>;
    */

    fn pop_front_pending_backwards(
        &mut self,
        friend_public_key: PublicKey,
    ) -> AsyncOpResult<Option<(Currency, BackwardsOp)>>;

    fn push_back_pending_backwards(
        &mut self,
        friend_public_key: PublicKey,
        currency: Currency,
        backwards_op: BackwardsOp,
    ) -> AsyncOpResult<()>;

    fn pop_front_pending_user_requests(
        &mut self,
        friend_public_key: PublicKey,
    ) -> AsyncOpResult<Option<(Currency, RequestSendFundsOp)>>;

    fn push_back_pending_user_requests(
        &mut self,
        friend_public_key: PublicKey,
        currency: Currency,
        request_op: RequestSendFundsOp,
    ) -> AsyncOpResult<()>;

    fn pop_front_pending_requests(
        &mut self,
        friend_public_key: PublicKey,
    ) -> AsyncOpResult<Option<(Currency, RequestSendFundsOp)>>;

    fn push_back_pending_requests(
        &mut self,
        friend_public_key: PublicKey,
        currency: Currency,
        request_op: RequestSendFundsOp,
    ) -> AsyncOpResult<()>;
}

#[derive(Debug)]
pub struct SwitchOutput {
    pub friends_messages: HashMap<PublicKey, Vec<FriendMessage>>,
    pub index_mutations: Vec<IndexMutation>,
    pub updated_remote_relays: Vec<PublicKey>,
    pub incoming_requests: Vec<RequestSendFundsOp>,
    pub incoming_responses: Vec<ResponseSendFundsOp>,
    pub incoming_cancels: Vec<CancelSendFundsOp>,
}

impl SwitchOutput {
    pub fn new() -> Self {
        SwitchOutput {
            friends_messages: HashMap::new(),
            index_mutations: Vec::new(),
            updated_remote_relays: Vec::new(),
            incoming_requests: Vec::new(),
            incoming_responses: Vec::new(),
            incoming_cancels: Vec::new(),
        }
    }

    pub fn add_friend_message(&mut self, public_key: PublicKey, friend_message: FriendMessage) {
        let entry = self
            .friends_messages
            .entry(public_key)
            .or_insert(Vec::new());
        (*entry).push(friend_message);
    }

    pub fn add_index_mutation(&mut self, index_mutation: IndexMutation) {
        self.index_mutations.push(index_mutation);
    }

    // TODO: Add updated remote relays?

    pub fn add_incoming_request(&mut self, request: RequestSendFundsOp) {
        self.incoming_requests.push(request);
    }

    pub fn add_incoming_response(&mut self, response: ResponseSendFundsOp) {
        self.incoming_responses.push(response);
    }

    pub fn add_incoming_cancel(&mut self, cancel: CancelSendFundsOp) {
        self.incoming_cancels.push(cancel);
    }
}
