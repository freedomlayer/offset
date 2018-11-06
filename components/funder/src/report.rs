use im::hashmap::HashMap as ImHashMap;

use crypto::identity::PublicKey;

use super::friend::{FriendState, ChannelStatus, ChannelInconsistent};
use super::state::FunderState;
use super::types::{RequestsStatus, FriendStatus, ResetTerms};
use super::mutual_credit::types::{TcBalance, TcRequestsStatus, MutualCredit};
use super::token_channel::TcDirection; 

#[derive(Clone)]
pub struct McReport {
    pub balance: TcBalance,
    pub requests_status: TcRequestsStatus,
}

#[derive(Clone)]
pub enum DirectionReport {
    Incoming,
    Outgoing,
}

#[derive(Clone)]
pub struct TcReport {
    pub direction: DirectionReport,
    // Equals Sha512/256(FriendMoveToken)
    pub mutual_credit: McReport,
}

#[derive(Clone)]
pub enum ChannelStatusReport {
    Inconsistent(ChannelInconsistent),
    Consistent(TcReport),
}

#[derive(Clone)]
pub struct FriendReport<A> {
    pub remote_address: A, 
    pub channel_status: ChannelStatusReport,
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

fn create_tc_report(mutual_credit: &MutualCredit) -> McReport {
    McReport {
        balance: mutual_credit.state().balance.clone(),
        requests_status: mutual_credit.state().requests_status.clone(),
    }
}

fn create_friend_report<A: Clone>(friend_state: &FriendState<A>) -> FriendReport<A> {
    let channel_status = match &friend_state.channel_status {
        ChannelStatus::Inconsistent(channel_inconsistent) => ChannelStatusReport::Inconsistent(channel_inconsistent.clone()),
        ChannelStatus::Consistent(token_channel) => {
            let direction = match token_channel.get_direction() {
                TcDirection::Incoming(_) => DirectionReport::Incoming,
                TcDirection::Outgoing(_) => DirectionReport::Outgoing,
            };
            let tc_report = TcReport {
                direction,
                mutual_credit: create_tc_report(&token_channel.get_mutual_credit()),
            };
            ChannelStatusReport::Consistent(tc_report)
        },
    };



    FriendReport {
        remote_address: friend_state.remote_address.clone(),
        channel_status,
        wanted_remote_max_debt: friend_state.wanted_remote_max_debt,
        wanted_local_requests_status: friend_state.wanted_local_requests_status.clone(),
        num_pending_responses: friend_state.pending_responses.len(),
        num_pending_requests: friend_state.pending_requests.len(),
        status: friend_state.status.clone(),
        num_pending_user_requests: friend_state.pending_user_requests.len(),
    }
}

pub fn create_report<A: Clone>(funder_state: &FunderState<A>) -> FunderReport<A> {
    let mut friends = ImHashMap::new();
    for (friend_public_key, friend_state) in &funder_state.friends {
        let friend_report = create_friend_report(&friend_state);
        friends.insert(friend_public_key.clone(), friend_report);
    }

    FunderReport {
        friends,
        num_ready_receipts: funder_state.ready_receipts.len(),
        local_public_key: funder_state.local_public_key.clone(),
    }

}

