use std::fmt::Debug;

use signature::canonical::CanonicalSerialize;

use proto::funder::messages::{Currency, PendingTransaction};

use proto::crypto::{PublicKey, Uid};

use crate::state::FunderState;

use crate::ephemeral::Ephemeral;
use crate::friend::ChannelStatus;

/// Find the originator of a pending local request.
/// This should be a pending remote request at some other friend.
/// Returns the public key of a friend. If we are the origin of this request, the function returns None.
///
/// TODO: We need to change this search to be O(1) in the future. Possibly by maintaining a map
/// between request_id and (friend_public_key, friend).
pub fn find_request_origin<'a, B>(
    state: &'a FunderState<B>,
    currency: &Currency,
    request_id: &Uid,
) -> Option<&'a PublicKey>
where
    B: Clone + CanonicalSerialize + PartialEq + Eq + Debug,
{
    for (friend_public_key, friend) in &state.friends {
        match &friend.channel_status {
            ChannelStatus::Inconsistent(_) => continue,
            ChannelStatus::Consistent(channel_consistent) => {
                let mutual_credit = if let Some(mutual_credit) = channel_consistent
                    .token_channel
                    .get_mutual_credits()
                    .get(currency)
                {
                    mutual_credit
                } else {
                    continue;
                };

                if mutual_credit
                    .state()
                    .pending_transactions
                    .remote
                    .contains_key(request_id)
                {
                    return Some(friend_public_key);
                }
            }
        }
    }
    None
}

/// Find an outgoing pending transaction
pub fn find_local_pending_transaction<'a, B>(
    state: &'a FunderState<B>,
    currency: &Currency,
    request_id: &Uid,
) -> Option<&'a PendingTransaction>
where
    B: Clone + CanonicalSerialize + PartialEq + Eq + Debug,
{
    for (_friend_public_key, friend) in &state.friends {
        match &friend.channel_status {
            ChannelStatus::Inconsistent(_) => continue,
            ChannelStatus::Consistent(channel_consistent) => {
                let mutual_credit = if let Some(mutual_credit) = channel_consistent
                    .token_channel
                    .get_mutual_credits()
                    .get(currency)
                {
                    mutual_credit
                } else {
                    continue;
                };

                if let Some(pending_transaction) = mutual_credit
                    .state()
                    .pending_transactions
                    .local
                    .get(request_id)
                {
                    return Some(pending_transaction);
                }
            }
        }
    }
    None
}

/// Find an incoming pending transaction
pub fn find_remote_pending_transaction<'a, B>(
    state: &'a FunderState<B>,
    currency: &Currency,
    request_id: &Uid,
) -> Option<&'a PendingTransaction>
where
    B: Clone + CanonicalSerialize + PartialEq + Eq + Debug,
{
    for (_friend_public_key, friend) in &state.friends {
        match &friend.channel_status {
            ChannelStatus::Inconsistent(_) => continue,
            ChannelStatus::Consistent(channel_consistent) => {
                let mutual_credit = if let Some(mutual_credit) = channel_consistent
                    .token_channel
                    .get_mutual_credits()
                    .get(currency)
                {
                    mutual_credit
                } else {
                    continue;
                };

                if let Some(pending_transaction) = mutual_credit
                    .state()
                    .pending_transactions
                    .remote
                    .get(request_id)
                {
                    return Some(pending_transaction);
                }
            }
        }
    }
    None
}

pub fn is_friend_ready<B>(
    state: &FunderState<B>,
    ephemeral: &Ephemeral,
    friend_public_key: &PublicKey,
    currency: &Currency,
) -> bool
where
    B: Clone + CanonicalSerialize + PartialEq + Eq + Debug,
{
    let friend = state.friends.get(friend_public_key).unwrap();
    if !ephemeral.liveness.is_online(friend_public_key) {
        return false;
    }

    // Make sure that the channel is consistent:
    let token_channel = match &friend.channel_status {
        ChannelStatus::Inconsistent(_) => return false,
        ChannelStatus::Consistent(channel_consistent) => &channel_consistent.token_channel,
    };

    let mutual_credit =
        if let Some(mutual_credit) = token_channel.get_mutual_credits().get(currency) {
            mutual_credit
        } else {
            // TODO: Possibly a different error message for this case? Maybe to be added externally at
            // the call site? The currency does not exist!
            return false;
        };

    // Make sure that the remote side has open requests:
    mutual_credit.state().requests_status.remote.is_open()
}
