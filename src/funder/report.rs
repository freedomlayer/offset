use im::hashmap::HashMap as ImHashMap;

use crypto::identity::PublicKey;

use super::friend::{FriendState, InconsistencyStatus};
use super::state::FunderState;
use super::types::{RequestsStatus, FriendStatus};
use super::token_channel::types::{TcBalance, TcRequestsStatus};

#[derive(Clone)]
pub struct TcReport {
    pub balance: TcBalance,
    pub requests_status: TcRequestsStatus,
}

#[derive(Clone)]
pub enum DirectionReport {
    Incoming,
    Outgoing,
}

#[derive(Clone)]
pub struct DirectionalTcReport {
    pub direction: DirectionReport,
    // Equals Sha512/256(FriendMoveToken)
    pub token_channel: TcReport,
}

#[derive(Clone)]
pub struct FriendReport<A> {
    pub opt_remote_address: Option<A>, 
    pub directional: DirectionalTcReport,
    pub inconsistency_status: InconsistencyStatus,
    pub wanted_remote_max_debt: u128,
    pub wanted_local_requests_status: RequestsStatus,
    pub num_pending_responses: usize,
    pub num_pending_requests: usize,
    // Pending operations to be sent to the token channel.
    pub status: FriendStatus,
    pub num_pending_user_requests: usize,
    // Request that the user has sent to this neighbor, 
    // but have not been processed yet. Bounded in size.
}

pub struct FunderReport<A: Clone> {
    pub friends: ImHashMap<PublicKey, FriendReport<A>>,
    pub num_ready_receipts: usize,
    pub local_public_key: PublicKey,

}

fn create_friend_report<A: Clone>(friend_state: &FriendState<A>) -> FriendReport<A> {
    unimplemented!();
}

pub fn create_report<A: Clone>(funder_state: &FunderState<A>) -> FunderReport<A> {
    let friends = ImHashMap::new();
    // TODO: Convert all friend_state-s into friend_report-s
    unimplemented!();

    FunderReport {
        friends,
        num_ready_receipts: funder_state.ready_receipts.len(),
        local_public_key: funder_state.local_public_key.clone(),
    }

}

