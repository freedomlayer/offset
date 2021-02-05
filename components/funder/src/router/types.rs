use std::collections::HashMap;

use derive_more::From;

use common::async_rpc::{AsyncOpResult, AsyncOpStream, OpError};

use crate::liveness::Liveness;
use crate::token_channel::TcDbClient;

use proto::app_server::messages::{NamedRelayAddress, RelayAddressPort};
use proto::crypto::{PublicKey, Uid};
use proto::funder::messages::{
    CancelSendFundsOp, Currency, FriendMessage, McBalance, Rate, RequestSendFundsOp,
    ResponseSendFundsOp,
};
use proto::index_server::messages::IndexMutation;

use crate::mutual_credit::{McCancel, McRequest, McResponse};
use crate::token_channel::TokenChannelError;

#[derive(Debug, From)]
pub enum RouterError {
    FriendAlreadyOnline,
    FriendAlreadyOffline,
    GenerationOverflow,
    BalanceOverflow,
    InvalidRoute,
    InvalidState,
    UnexpectedTcStatus,
    TokenChannelError(TokenChannelError),
    OpError(OpError),
}

/// Router's ephemeral state (Not saved inside the database)
#[derive(Debug)]
pub struct RouterState {
    pub liveness: Liveness,
}

#[derive(Debug)]
pub enum BackwardsOp {
    Response(Currency, McResponse),
    Cancel(Currency, McCancel),
}

#[derive(Debug)]
pub struct CurrencyInfo {
    /// Currency rate: This is how much it costs to the remote friend to send credits through us.
    pub rate: Rate,
    /// Maximum amount of debt we allow to the remote side. This is the maximum rich we can get
    /// from this currency relationship.
    pub remote_max_debt: u128,
    /// Maximum amount of debt we are willing to get into.
    pub local_max_debt: u128,
    /// Do we allow requests to go through this currency?
    /// TODO: Find out exactly what this means.
    pub is_open: bool,
    /// Is locally marked for removal?
    pub is_remove: bool,
    /// Is mutual credit open? Show balances
    pub opt_mutual_credit: Option<McBalance>,
}

/*
#[derive(Debug)]
pub struct FriendInfo {
    pub public_key: PublicKey,
    pub name: PublicKey,
    pub is_enabled: bool,
}
*/

#[derive(Debug)]
pub struct RequestOrigin {
    pub friend_public_key: PublicKey,
    pub currency: Currency,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SentRelay {
    pub relay_address: RelayAddressPort,
    pub is_remove: bool,
    pub opt_generation: Option<u128>,
}

pub trait RouterDbClient {
    type TcDbClient: TcDbClient;
    fn tc_db_client(
        &mut self,
        friend_public_key: PublicKey,
    ) -> AsyncOpResult<Option<&mut Self::TcDbClient>>;

    /*
    /// Get the current list of local relays
    fn get_local_relays(&mut self) -> AsyncOpResult<Vec<NamedRelayAddress>>;
    */

    /// A util to iterate over friends and mutating them.
    /// A None input will return the first friend.
    /// A None output means that there are no more friends.
    fn get_next_friend(
        &mut self,
        prev_friend_public_key: Option<PublicKey>,
    ) -> AsyncOpResult<Option<PublicKey>>;

    /// Get the last set of sent relays, together with their generation
    /// Generation == None means that the sent relays were already acked
    fn get_last_sent_relays(
        &mut self,
        friend_public_key: PublicKey,
    ) -> AsyncOpResult<(Option<u128>, Vec<RelayAddressPort>)>;

    /// Get detailed list of sent relays, including generation information
    fn get_sent_relays(&mut self, friend_public_key: PublicKey) -> AsyncOpResult<(Vec<SentRelay>)>;

    /// Set list of sent relays
    fn set_sent_relays(
        &mut self,
        friend_public_key: PublicKey,
        sent_relays: Vec<SentRelay>,
    ) -> AsyncOpResult<()>;

    /*
    /// Update sent relays for friend `friend_public_key` with the list of current local relays.
    fn update_sent_relays(
        &mut self,
        friend_public_key: PublicKey,
        generation: u128,
        sent_relays: Vec<RelayAddress>,
    ) -> AsyncOpResult<()>;
    */

    /*
    fn get_balance(&mut self) -> AsyncOpResult<McBalance>;
    */

    /*
    fn peek_pending_backwards(
        &mut self,
        friend_public_key: PublicKey,
    ) -> AsyncOpResult<BackwardsOp>;
    */

    fn add_local_request(&mut self, request_id: Uid) -> AsyncOpResult<()>;
    fn remove_local_request(&mut self, request_id: Uid) -> AsyncOpResult<()>;

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
    ) -> AsyncOpResult<Option<(Currency, McRequest)>>;

    fn pending_user_requests_push_back(
        &mut self,
        friend_public_key: PublicKey,
        currency: Currency,
        mc_request: McRequest,
    ) -> AsyncOpResult<()>;

    fn pending_user_requests_is_empty(
        &mut self,
        friend_public_key: PublicKey,
    ) -> AsyncOpResult<bool>;

    fn pending_requests_pop_front(
        &mut self,
        friend_public_key: PublicKey,
    ) -> AsyncOpResult<Option<(Currency, McRequest)>>;

    /// Check if a `request_id` is already in use.
    /// Searches all local pending requests, and all local open transactions.
    fn is_local_request_exists(&mut self, request_id: Uid) -> AsyncOpResult<bool>;

    // TODO: What happens if impossible to push, due to duplicate request id?
    fn pending_requests_push_back(
        &mut self,
        friend_public_key: PublicKey,
        currency: Currency,
        mc_request: McRequest,
    ) -> AsyncOpResult<()>;

    fn pending_requests_is_empty(&mut self, friend_public_key: PublicKey) -> AsyncOpResult<bool>;

    fn list_open_currencies(&mut self, friend_public_key: PublicKey)
        -> AsyncOpStream<CurrencyInfo>;

    fn get_currency_info(
        &mut self,
        friend_public_key: PublicKey,
        currency: Currency,
    ) -> AsyncOpResult<Option<CurrencyInfo>>;

    /// Check if the origin of the request is any of the friends
    /// If so, find the relevant friend
    fn get_remote_pending_request_origin(
        &mut self,
        request_id: Uid,
    ) -> AsyncOpResult<Option<RequestOrigin>>;

    /// Check if this request has originated from the local node
    fn is_request_local_origin(&mut self, request_id: Uid) -> AsyncOpResult<bool>;

    fn add_currency_config(
        &mut self,
        friend_public_key: PublicKey,
        currency: Currency,
    ) -> AsyncOpResult<()>;

    /// Set currency to be removed when possible
    fn set_currency_remove(
        &mut self,
        friend_public_key: PublicKey,
        currency: Currency,
    ) -> AsyncOpResult<()>;

    /// Unset currency removal
    fn unset_currency_remove(
        &mut self,
        friend_public_key: PublicKey,
        currency: Currency,
    ) -> AsyncOpResult<()>;

    fn set_remote_max_debt(
        &mut self,
        friend_public_key: PublicKey,
        currency: Currency,
        remote_max_debt: u128,
    ) -> AsyncOpResult<()>;

    fn open_currency(
        &mut self,
        friend_public_key: PublicKey,
        currency: Currency,
    ) -> AsyncOpResult<()>;

    fn close_currency(
        &mut self,
        friend_public_key: PublicKey,
        currency: Currency,
    ) -> AsyncOpResult<()>;

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
    pub incoming_requests: Vec<McRequest>,
    pub incoming_responses: Vec<McResponse>,
    pub incoming_cancels: Vec<McCancel>,
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

    pub fn add_incoming_request(&mut self, request: McRequest) {
        self.incoming_requests.push(request);
    }

    pub fn add_incoming_response(&mut self, response: McResponse) {
        self.incoming_responses.push(response);
    }

    pub fn add_incoming_cancel(&mut self, cancel: McCancel) {
        self.incoming_cancels.push(cancel);
    }
}
