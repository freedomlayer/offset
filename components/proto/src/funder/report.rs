use im::hashmap::HashMap as ImHashMap;

use crypto::hash::HashResult;
use crypto::crypto_rand::RandValue;

use crate::funder::messages::{RequestsStatus, FriendStatus, TPublicKey, TSignature};

#[derive(Clone, Debug)]
pub struct MoveTokenHashedReport<P,MS> {
    pub prefix_hash: HashResult,
    pub local_public_key: TPublicKey<P>,
    pub remote_public_key: TPublicKey<P>,
    pub inconsistency_counter: u64,
    pub move_token_counter: u128,
    pub balance: i128,
    pub local_pending_debt: u128,
    pub remote_pending_debt: u128,
    pub rand_nonce: RandValue,
    pub new_token: TSignature<MS>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
pub enum SentLocalAddressReport<A> {
    NeverSent,
    Transition((A, A)), // (last sent, before last sent)
    LastSent(A),
}

#[derive(Clone, Serialize, Deserialize, Debug, Eq, PartialEq)]
pub enum FriendStatusReport {
    Enabled = 1,
    Disabled = 0,
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize, Debug)]
pub enum RequestsStatusReport {
    Open,
    Closed,
}


#[derive(Eq, PartialEq, Clone, Serialize, Deserialize, Debug)]
pub struct McRequestsStatusReport {
    // Local is open/closed for incoming requests:
    pub local: RequestsStatusReport,
    // Remote is open/closed for incoming requests:
    pub remote: RequestsStatusReport,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct McBalanceReport {
    /// Amount of credits this side has against the remote side.
    /// The other side keeps the negation of this value.
    pub balance: i128,
    /// Maximum possible remote debt
    pub remote_max_debt: u128,
    /// Maximum possible local debt
    pub local_max_debt: u128,
    /// Frozen credits by our side
    pub local_pending_debt: u128,
    /// Frozen credits by the remote side
    pub remote_pending_debt: u128,
}

#[derive(Clone, Debug)]
pub enum DirectionReport {
    Incoming,
    Outgoing,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum FriendLivenessReport {
    Online,
    Offline,
}

#[derive(Clone, Debug)]
pub struct TcReport {
    pub direction: DirectionReport,
    pub balance: McBalanceReport,
    pub requests_status: McRequestsStatusReport,
    pub num_local_pending_requests: u64,
    pub num_remote_pending_requests: u64,
}

#[derive(Clone, Debug)]
pub struct ResetTermsReport<MS> {
    pub reset_token: TSignature<MS>,
    pub balance_for_reset: i128,
}

#[derive(Clone, Debug)]
pub struct ChannelInconsistentReport<MS> {
    pub local_reset_terms_balance: i128,
    pub opt_remote_reset_terms: Option<ResetTermsReport<MS>>,
}

#[derive(Clone, Debug)]
pub enum ChannelStatusReport<MS> {
    Inconsistent(ChannelInconsistentReport<MS>),
    Consistent(TcReport),
}

#[derive(Clone, Debug)]
pub struct FriendReport<A,P,MS> {
    pub remote_address: A, 
    pub name: String,
    pub sent_local_address: SentLocalAddressReport<A>,
    // Last message signed by the remote side. 
    // Can be used as a proof for the last known balance.
    pub opt_last_incoming_move_token: Option<MoveTokenHashedReport<P,MS>>,
    pub liveness: FriendLivenessReport, // is the friend online/offline?
    pub channel_status: ChannelStatusReport<MS>,
    pub wanted_remote_max_debt: u128,
    pub wanted_local_requests_status: RequestsStatusReport,
    pub num_pending_requests: u64,
    pub num_pending_responses: u64,
    // Pending operations to be sent to the token channel.
    pub status: FriendStatusReport,
    pub num_pending_user_requests: u64,
    // Request that the user has sent to this neighbor, 
    // but have not been processed yet. Bounded in size.
}

/// A FunderReport is a summary of a FunderState.
/// It contains the information the Funder exposes to the user apps of the Offst node.
#[derive(Clone)]
// TODO: Removed A: Clone here and ImHashMap. Should this struct be cloneable for some reason?
pub struct FunderReport<A:Clone,P:Clone,MS:Clone> {
    pub local_public_key: TPublicKey<P>,
    pub opt_address: Option<A>,
    pub friends: ImHashMap<TPublicKey<P>, FriendReport<A,P,MS>>,
    pub num_ready_receipts: u64,
}

#[allow(unused)]
#[derive(Debug)]
pub enum FriendReportMutation<A,P,MS> {
    SetRemoteAddress(A),
    SetName(String),
    SetSentLocalAddress(SentLocalAddressReport<A>),
    SetChannelStatus(ChannelStatusReport<MS>),
    SetWantedRemoteMaxDebt(u128),
    SetWantedLocalRequestsStatus(RequestsStatusReport),
    SetNumPendingRequests(u64),
    SetNumPendingResponses(u64),
    SetFriendStatus(FriendStatusReport),
    SetNumPendingUserRequests(u64),
    SetOptLastIncomingMoveToken(Option<MoveTokenHashedReport<P,MS>>),
    SetLiveness(FriendLivenessReport),
}

#[derive(Clone, Debug)]
pub struct AddFriendReport<A,P,MS> {
    pub friend_public_key: TPublicKey<P>,
    pub address: A,
    pub name: String,
    pub balance: i128, // Initial balance
    pub opt_last_incoming_move_token: Option<MoveTokenHashedReport<P,MS>>,
    pub channel_status: ChannelStatusReport<MS>,
}


#[allow(unused)]
#[derive(Debug)]
pub enum FunderReportMutation<A,P,MS> {
    SetAddress(Option<A>),
    AddFriend(AddFriendReport<A,P,MS>),
    RemoveFriend(TPublicKey<P>),
    FriendReportMutation((TPublicKey<P>, FriendReportMutation<A,P,MS>)),
    SetNumReadyReceipts(u64),
}


impl From<&FriendStatus> for FriendStatusReport {
    fn from(friend_status: &FriendStatus) -> FriendStatusReport {
        match friend_status {
            FriendStatus::Enabled => FriendStatusReport::Enabled,
            FriendStatus::Disabled => FriendStatusReport::Disabled,
        }
    }
}


impl From<&RequestsStatus> for RequestsStatusReport {
    fn from(requests_status: &RequestsStatus) -> RequestsStatusReport {
        match requests_status {
            RequestsStatus::Open => RequestsStatusReport::Open,
            RequestsStatus::Closed => RequestsStatusReport::Closed,
        }
    }
}

