use common::async_rpc::{AsyncOpResult, AsyncOpStream};
use std::collections::HashMap;
// use common::ser_utils::ser_string;
// use common::u256::U256;

use crate::liveness::Liveness;
use crate::token_channel::TcDbClient;

use proto::app_server::messages::{NamedRelayAddress, RelayAddress};
use proto::crypto::{PublicKey, Uid};
use proto::funder::messages::{
    CancelSendFundsOp, Currency, FriendMessage, McBalance, Rate, RequestSendFundsOp,
    ResponseSendFundsOp,
};
use proto::index_server::messages::IndexMutation;

/// Router's ephemeral state (Not saved inside the database)
#[derive(Debug)]
pub struct RouterState {
    pub liveness: Liveness,
}

#[derive(Debug)]
pub enum BackwardsOp {
    Response(ResponseSendFundsOp),
    Cancel(CancelSendFundsOp),
}

#[derive(Debug)]
pub struct CurrencyInfoLocal {
    /// Is locally marked for removal?
    pub is_remove: bool,
    /// Was set by remote side too?
    pub opt_remote: Option<McBalance>,
}

#[derive(Debug)]
pub struct CurrencyInfo {
    /// Currency name
    pub currency: Currency,
    /// Currency rate: This is how much it costs to the remote friend to send credits through us.
    pub rate: Rate,
    /// Maximum amount of debt we allow to the remote side. This is the maximum rich we can get
    /// from this currency relationship.
    pub remote_max_debt: u128,
    /// Do we allow requests to go through this currency?
    /// TODO: Find out exactly what this means.
    pub is_open: bool,
    /// Was the remote side told about this currency?
    pub opt_local: Option<CurrencyInfoLocal>,
}

#[derive(Debug)]
pub struct RequestOrigin {
    pub friend_public_key: PublicKey,
    pub currency: Currency,
}

pub trait RouterDbClient {
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

    fn pending_backwards_pop_front(
        &mut self,
        friend_public_key: PublicKey,
    ) -> AsyncOpResult<Option<(Currency, BackwardsOp)>>;

    fn pending_backwards_push_back(
        &mut self,
        friend_public_key: PublicKey,
        currency: Currency,
        backwards_op: BackwardsOp,
    ) -> AsyncOpResult<()>;

    fn pending_backwards_is_empty(&mut self, friend_public_key: PublicKey) -> AsyncOpResult<bool>;

    fn pending_user_requests_pop_front(
        &mut self,
        friend_public_key: PublicKey,
    ) -> AsyncOpResult<Option<(Currency, RequestSendFundsOp)>>;

    fn pending_user_requests_push_back(
        &mut self,
        friend_public_key: PublicKey,
        currency: Currency,
        request_op: RequestSendFundsOp,
    ) -> AsyncOpResult<()>;

    fn pending_user_requests_is_empty(
        &mut self,
        friend_public_key: PublicKey,
    ) -> AsyncOpResult<bool>;

    fn pending_requests_pop_front(
        &mut self,
        friend_public_key: PublicKey,
    ) -> AsyncOpResult<Option<(Currency, RequestSendFundsOp)>>;

    fn pending_requests_push_back(
        &mut self,
        friend_public_key: PublicKey,
        currency: Currency,
        request_op: RequestSendFundsOp,
    ) -> AsyncOpResult<()>;

    fn pending_requests_is_empty(&mut self, friend_public_key: PublicKey) -> AsyncOpResult<bool>;

    fn list_open_currencies(&mut self, friend_public_key: PublicKey)
        -> AsyncOpStream<CurrencyInfo>;

    fn get_currency_info(
        &mut self,
        friend_public_key: PublicKey,
        currency: Currency,
    ) -> AsyncOpResult<Option<CurrencyInfo>>;

    fn get_remote_pending_request_origin(
        &mut self,
        request_id: Uid,
    ) -> AsyncOpResult<Option<RequestOrigin>>;

    /*
    /// Get a list of configured currencies that were not yet added as local currencies
    fn currencies_to_add(&mut self, friend_public_key: PublicKey) -> AsyncOpResult<Vec<Currency>>;

    /// Get a list of local currencies that can be removed at the moment. Options:
    /// - Not present in remote currencies
    /// - Present in remote currencies but have a zero balance
    fn currencies_to_remove(
        &mut self,
        friend_public_key: PublicKey,
    ) -> AsyncOpResult<Vec<Currency>>;
    */

    /// Add currencies
    /// --------------
    /// Get a list of configured currencies that were not yet added as local currencies
    ///
    /// Remove currencies
    /// -----------------
    /// Get a list of local currencies that can be removed at the moment. Options:
    /// - Not present in remote currencies
    /// - Present in remote currencies but have a zero balance
    fn currencies_diff(&mut self, friend_public_key: PublicKey) -> AsyncOpResult<Vec<Currency>>;
}

#[derive(Debug)]
pub struct RouterOutput {
    pub friends_messages: HashMap<PublicKey, Vec<FriendMessage>>,
    pub index_mutations: Vec<IndexMutation>,
    pub updated_remote_relays: Vec<PublicKey>,
    pub incoming_requests: Vec<RequestSendFundsOp>,
    pub incoming_responses: Vec<ResponseSendFundsOp>,
    pub incoming_cancels: Vec<CancelSendFundsOp>,
}

impl RouterOutput {
    pub fn new() -> Self {
        RouterOutput {
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
