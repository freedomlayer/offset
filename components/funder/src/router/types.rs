use std::collections::{HashMap, HashSet};

use derive_more::From;

use common::async_rpc::{AsyncOpResult, AsyncOpStream, OpError};
use common::u256::U256;

use crypto::rand::CryptoRandom;
use identity::IdentityClient;

use database::transaction::Transaction;

use proto::app_server::messages::{NamedRelayAddress, RelayAddressPort};
use proto::crypto::{PublicKey, Uid};
use proto::funder::messages::{
    CancelSendFundsOp, Currency, FriendMessage, McBalance, Rate, RequestSendFundsOp,
    ResponseSendFundsOp,
};
use proto::index_server::messages::IndexMutation;
use proto::net::messages::NetAddress;

use crate::liveness::Liveness;
use crate::mutual_credit::{McCancel, McDbClient, McRequest, McResponse};
use crate::token_channel::TcDbClient;
use crate::token_channel::TokenChannelError;

#[derive(Debug, From)]
pub enum RouterError {
    FriendAlreadyOnline,
    FriendAlreadyOffline,
    GenerationOverflow,
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
pub struct FriendBalance {
    /// Amount of credits this side has against the remote side.
    /// The other side keeps the negation of this value.
    pub balance: i128,
    /// Fees that were received from remote side
    pub in_fees: U256,
    /// Fees that were given to remote side
    pub out_fees: U256,
}

#[derive(Debug)]
pub struct FriendBalanceDiff {
    /// Old balance (Before event occured)
    pub old_balance: FriendBalance,
    /// New balance (after event occured)
    pub new_balance: FriendBalance,
}

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

    /// Add a new friend
    fn add_friend(
        &mut self,
        friend_name: String,
        friend_public_key: PublicKey,
    ) -> AsyncOpResult<()>;

    /// Remove friend
    fn remove_friend(&mut self, friend_public_key: PublicKey) -> AsyncOpResult<()>;

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
    ) -> AsyncOpResult<Option<BackwardsOp>>;

    fn pending_backwards_push_back(
        &mut self,
        friend_public_key: PublicKey,
        backwards_op: BackwardsOp,
    ) -> AsyncOpResult<()>;

    fn pending_backwards_is_empty(&mut self, friend_public_key: PublicKey) -> AsyncOpResult<bool>;

    fn pending_user_requests_pop_front(
        &mut self,
        friend_public_key: PublicKey,
    ) -> AsyncOpResult<Option<(Currency, McRequest)>>;

    fn pending_user_requests_pop_front_by_currency(
        &mut self,
        friend_public_key: PublicKey,
    ) -> AsyncOpResult<Option<McRequest>>;

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

    fn pending_requests_pop_front_by_currency(
        &mut self,
        friend_public_key: PublicKey,
        currency: Currency,
    ) -> AsyncOpResult<Option<McRequest>>;

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

    fn list_open_currencies(
        &mut self,
        friend_public_key: PublicKey,
    ) -> AsyncOpStream<(Currency, CurrencyInfo)>;

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

    fn set_local_max_debt(
        &mut self,
        friend_public_key: PublicKey,
        currency: Currency,
        local_max_debt: u128,
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

    fn remove_currency(
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

    /// Add a friend event
    /// Can be one of: 1. Channel reset, 2. Friend removal, 3. currency removal
    fn add_friend_event(
        &mut self,
        friend_public_key: PublicKey,
        balances_diff: HashMap<Currency, FriendBalanceDiff>,
    ) -> AsyncOpResult<()>;
}

#[derive(Debug)]
pub struct RouterOutput {
    pub index_mutations: Vec<IndexMutation>,
    // pub updated_remote_relays: Vec<PublicKey>,
    pub incoming_requests: Vec<(Currency, McRequest)>,
    pub incoming_responses: Vec<(Currency, McResponse)>,
    pub incoming_cancels: Vec<(Currency, McCancel)>,
}

impl RouterOutput {
    pub fn new() -> Self {
        RouterOutput {
            index_mutations: Vec::new(),
            // updated_remote_relays: Vec::new(),
            incoming_requests: Vec::new(),
            incoming_responses: Vec::new(),
            incoming_cancels: Vec::new(),
        }
    }

    pub fn add_index_mutation(&mut self, index_mutation: IndexMutation) {
        self.index_mutations.push(index_mutation);
    }

    // TODO: Add updated remote relays?

    pub fn add_incoming_request(&mut self, currency: Currency, request: McRequest) {
        self.incoming_requests.push((currency, request));
    }

    pub fn add_incoming_response(&mut self, currency: Currency, response: McResponse) {
        self.incoming_responses.push((currency, response));
    }

    pub fn add_incoming_cancel(&mut self, currency: Currency, cancel: McCancel) {
        self.incoming_cancels.push((currency, cancel));
    }
}

#[derive(Debug)]
pub struct RouterInfo {
    pub local_public_key: PublicKey,
    pub max_operations_in_batch: usize,
}

#[derive(Debug)]
pub struct SendCommands {
    /// friend_public_key -> allow_empty
    move_token: HashMap<PublicKey, bool>,
    /// Should we send relays update to remote side?
    relays_update: HashSet<PublicKey>,
}

impl SendCommands {
    pub fn new() -> Self {
        Self {
            move_token: HashMap::new(),
            relays_update: HashSet::new(),
        }
    }

    /// Schedule send for a friend.
    pub fn move_token(&mut self, friend_public_key: PublicKey) {
        // Note: If we already set allow_empty = true, we will leave it equals to true.
        let _ = self.move_token.entry(friend_public_key).or_insert(false);
    }

    /// Schedule a send for a friend. Send will happen even if there is nothing to send.
    pub fn move_token_allow_empty(&mut self, friend_public_key: PublicKey) {
        let _ = self.move_token.insert(friend_public_key, true);
    }

    /// Schedule relays update message
    pub fn relays_update(&mut self, friend_public_key: PublicKey) {
        self.relays_update.insert(friend_public_key);
    }
}

#[derive(Debug)]
pub struct RouterHandle<'a, R, RC> {
    pub router_db_client: &'a mut RC,
    pub identity_client: &'a mut IdentityClient,
    pub rng: &'a mut R,
    /// Friends with new pending outgoing messages
    pub send_commands: SendCommands,
    /// Ephemeral state:
    pub ephemeral: &'a mut RouterState,
    /// Router's output:
    pub output: RouterOutput,
}

#[derive(Debug)]
pub struct RouterAccess<'a, R, RC> {
    pub router_db_client: &'a mut RC,
    pub identity_client: &'a mut IdentityClient,
    pub rng: &'a mut R,
    /// Friends with new pending outgoing messages
    pub send_commands: &'a mut SendCommands,
    /// Ephemeral state:
    pub ephemeral: &'a mut RouterState,
    /// Router's output:
    pub output: &'a mut RouterOutput,
}

pub trait RouterControl {
    type Rng: CryptoRandom;
    type RouterDbClient: RouterDbClient<TcDbClient = Self::TcDbClient>;
    type TcDbClient: TcDbClient<McDbClient = Self::McDbClient> + Transaction + Send;
    type McDbClient: McDbClient + Send;

    fn access(&mut self) -> RouterAccess<'_, Self::Rng, Self::RouterDbClient>;
}

impl<'a, R, RC> RouterControl for RouterHandle<'a, R, RC>
where
    RC: RouterDbClient,
    <RC as RouterDbClient>::TcDbClient: Transaction + Send,
    <<RC as RouterDbClient>::TcDbClient as TcDbClient>::McDbClient: Send,
    R: CryptoRandom,
{
    type Rng = R;
    type RouterDbClient = RC;
    type TcDbClient = <RC as RouterDbClient>::TcDbClient;
    type McDbClient = <<RC as RouterDbClient>::TcDbClient as TcDbClient>::McDbClient;

    fn access(&mut self) -> RouterAccess<'_, Self::Rng, Self::RouterDbClient> {
        RouterAccess {
            router_db_client: self.router_db_client,
            identity_client: self.identity_client,
            rng: self.rng,
            send_commands: &mut self.send_commands,
            ephemeral: self.ephemeral,
            output: &mut self.output,
        }
    }

    /*
    fn router_db_client(&mut self) -> &mut Self::RouterDbClient {
        self.router_db_client
    }
    fn identity_client(&mut self) -> &mut IdentityClient {
        self.identity_client
    }
    fn rng(&mut self) -> &mut Self::Rng {
        self.rng
    }
    fn add_pending_send(&mut self, friend_public_key: PublicKey) -> bool {
        self.pending_send.insert(friend_public_key)
    }
    fn ephemeral(&mut self) -> &mut RouterState {
        &mut self.state
    }
    fn output(&mut self) -> &mut RouterOutput {
        &mut self.output
    }
    */
}

#[derive(Debug)]
pub enum RouterOp {
    // Config
    // ------
    /// (friend_public_key, currency)
    AddCurrency(PublicKey, Currency),
    /// (friend_public_key, currency)
    SetRemoveCurrency(PublicKey, Currency),
    /// (friend_public_key, currency)
    UnsetRemoveCurrency(PublicKey, Currency),
    /// (friend_public_key, currency, remote_max_debt)
    SetRemoteMaxDebt(PublicKey, Currency, u128),
    /// (friend_public_key, currency, local_max_debt)
    SetLocalMaxDebt(PublicKey, Currency, u128),
    /// (friend_public_key, currency)
    OpenCurrency(PublicKey, Currency),
    /// (friend_public_key, currency)
    CloseCurrency(PublicKey, Currency),
    /// (friend_public_key, friend_name)
    AddFriend(PublicKey, String),
    /// (friend_public_key)
    RemoveFriend(PublicKey),
    // Friend
    // ------
    /// (friend_public_key, friend_message)
    FriendMessage(PublicKey, FriendMessage),
    // Livenesss
    // ---------
    /// (friend_public_key)
    SetFriendOnline(PublicKey),
    /// (friend_public_key)
    SetFriendOffline(PublicKey),
    // Relays
    // ------
    /// (friend_public_key, friend_local_relays)
    UpdateFriendLocalRelays(PublicKey, HashMap<PublicKey, NetAddress>),
    /// (friend_public_key, local_relays)
    UpdateLocalRelays(HashMap<PublicKey, NetAddress>),
    // Route
    // -----
    /// (currency, mc_request)
    SendRequest(Currency, McRequest),
    /// (mc_response)
    SendResponse(McResponse),
    /// (mc_cancel)
    SendCancel(McCancel),
}
