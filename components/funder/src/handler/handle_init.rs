use signature::canonical::CanonicalSerialize;
use std::fmt::Debug;

use proto::app_server::messages::RelayAddress;
use proto::funder::messages::{ChannelerUpdateFriend, FriendStatus};

use crate::handler::state_wrap::MutableFunderState;
use crate::types::ChannelerConfig;

pub fn handle_init<B>(
    m_state: &MutableFunderState<B>,
    outgoing_channeler_config: &mut Vec<ChannelerConfig<RelayAddress<B>>>,
) where
    B: Clone + CanonicalSerialize + PartialEq + Eq + Debug,
{
    let mut enabled_friends = Vec::new();
    for friend in m_state.state().friends.values() {
        match friend.status {
            FriendStatus::Enabled => {
                let channeler_add_friend = ChannelerUpdateFriend {
                    friend_public_key: friend.remote_public_key.clone(),
                    friend_relays: friend.remote_relays.clone(),
                    local_relays: friend.sent_local_relays.to_vec(),
                };
                enabled_friends.push(channeler_add_friend);
            }
            FriendStatus::Disabled => continue,
        };
    }

    // Send a report of the current FunderState:
    // This is a base report. Later reports are differential, and should be built on this base
    // report.
    // let report = create_report(&self.state, &self.ephemeral);
    // self.add_outgoing_control(FunderOutgoingControl::Report(report));

    // Notify Channeler about current address:
    let relays = m_state
        .state()
        .relays
        .iter()
        .cloned()
        .map(RelayAddress::from)
        .collect();
    outgoing_channeler_config.push(ChannelerConfig::SetRelays(relays));

    // Notify channeler about all enabled friends:
    for enabled_friend in enabled_friends {
        // Notify Channeler:
        outgoing_channeler_config.push(ChannelerConfig::UpdateFriend(enabled_friend));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use proto::crypto::PublicKey;
    use proto::funder::messages::AddFriend;

    use crate::friend::FriendMutation;
    use crate::state::{FunderMutation, FunderState};

    use crate::handler::state_wrap::MutableFunderState;
    use crate::handler::tests::utils::{dummy_named_relay_address, dummy_relay_address};

    #[test]
    fn test_handle_init_basic() {
        // let local_pk = identity_client.request_public_key().await.unwrap();
        let local_pk = PublicKey::from(&[0xbb; PublicKey::len()]);
        let pk_b = PublicKey::from(&[0xbb; PublicKey::len()]);

        let relays = vec![dummy_named_relay_address(0)];
        let mut state = FunderState::<u32>::new(local_pk, relays);

        // Add a remote friend:
        let add_friend = AddFriend {
            friend_public_key: pk_b.clone(),
            relays: vec![dummy_relay_address(3)],
            name: "pk_b".into(),
        };
        let f_mutation = FunderMutation::AddFriend(add_friend);
        state.mutate(&f_mutation);

        // Enable the remote friend:
        let friend_mutation = FriendMutation::SetStatus(FriendStatus::Enabled);
        let funder_mutation = FunderMutation::FriendMutation((pk_b.clone(), friend_mutation));
        state.mutate(&funder_mutation);

        let mut m_state = MutableFunderState::new(state);
        let mut outgoing_channeler_config = Vec::new();
        handle_init(&mut m_state, &mut outgoing_channeler_config);

        let (_initial_state, mutations, _final_state) = m_state.done();
        assert!(mutations.is_empty());
        // TODO: Check equality?
        // assert_eq!(initial_state, final_state);

        assert_eq!(outgoing_channeler_config.len(), 2);

        // SetAddress:
        let channeler_config = outgoing_channeler_config.remove(0);
        match channeler_config {
            ChannelerConfig::SetRelays(cur_relays) => {
                assert_eq!(cur_relays, vec![dummy_relay_address(0)]);
            }
            _ => unreachable!(),
        };

        // UpdateFriend:
        let channeler_config = outgoing_channeler_config.remove(0);
        match channeler_config {
            ChannelerConfig::UpdateFriend(channeler_update_friend) => {
                assert_eq!(
                    channeler_update_friend.friend_relays,
                    vec![dummy_relay_address(3)]
                );
                assert_eq!(channeler_update_friend.friend_public_key, pk_b);
            }
            _ => unreachable!(),
        };
    }
}
