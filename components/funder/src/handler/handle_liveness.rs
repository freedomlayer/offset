use std::fmt::Debug;

use signature::canonical::CanonicalSerialize;

use crypto::rand::CryptoRandom;

use proto::funder::messages::{FriendStatus, FunderOutgoingControl};

use crate::types::IncomingLivenessMessage;

use crate::ephemeral::EphemeralMutation;
use crate::liveness::LivenessMutation;

use crate::handler::canceler::{cancel_pending_requests, CurrencyChoice};
use crate::handler::state_wrap::{MutableEphemeral, MutableFunderState};
use crate::handler::types::SendCommands;

#[derive(Debug)]
pub enum HandleLivenessError {
    FriendDoesNotExist,
    FriendIsDisabled,
    FriendAlreadyOnline,
}

pub fn handle_liveness_message<B, R>(
    m_state: &mut MutableFunderState<B>,
    m_ephemeral: &mut MutableEphemeral,
    send_commands: &mut SendCommands,
    outgoing_control: &mut Vec<FunderOutgoingControl<B>>,
    rng: &R,
    liveness_message: IncomingLivenessMessage,
) -> Result<(), HandleLivenessError>
where
    B: Clone + CanonicalSerialize + PartialEq + Eq + Debug,
    R: CryptoRandom,
{
    match liveness_message {
        IncomingLivenessMessage::Online(friend_public_key) => {
            // Find friend:
            let friend = match m_state.state().friends.get(&friend_public_key) {
                Some(friend) => Ok(friend),
                None => Err(HandleLivenessError::FriendDoesNotExist),
            }?;
            match friend.status {
                FriendStatus::Enabled => Ok(()),
                FriendStatus::Disabled => Err(HandleLivenessError::FriendIsDisabled),
            }?;

            if m_ephemeral
                .ephemeral()
                .liveness
                .is_online(&friend_public_key)
            {
                // The liveness data in ephemeral represents the information we got from the
                // Channeler. We don't expect the channeler to send twice that a node is online.
                // This would mean a bug in the Channeler
                return Err(HandleLivenessError::FriendAlreadyOnline);
            }

            send_commands.set_resend_outgoing(&friend_public_key);

            let liveness_mutation = LivenessMutation::SetOnline(friend_public_key.clone());
            let ephemeral_mutation = EphemeralMutation::LivenessMutation(liveness_mutation);
            m_ephemeral.mutate(ephemeral_mutation);
        }
        IncomingLivenessMessage::Offline(friend_public_key) => {
            // It is possible that the friend is disabled and we get an offline notification.
            // This will usually happen if we just set the friend to be disabled. We will get the
            // offline notification for the friend short time after we set it to be disabled.
            let liveness_mutation = LivenessMutation::SetOffline(friend_public_key.clone());
            let ephemeral_mutation = EphemeralMutation::LivenessMutation(liveness_mutation);
            m_ephemeral.mutate(ephemeral_mutation);

            // If the friend does not exist, we have nothing more to do here:
            if m_state.state().friends.get(&friend_public_key).is_none() {
                return Ok(());
            }

            // Cancel all messages pending for this friend:
            cancel_pending_requests(
                m_state,
                send_commands,
                outgoing_control,
                rng,
                &friend_public_key,
                &CurrencyChoice::All,
            );
        }
    };
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::cmp::Ordering;

    use crypto::identity::{compare_public_key, Identity, SoftwareEd25519Identity};
    use crypto::rand::RandGen;
    use crypto::test_utils::DummyRandom;

    use proto::crypto::PrivateKey;
    use proto::funder::messages::{AddFriend, FriendStatus};

    use crate::ephemeral::Ephemeral;
    use crate::friend::{ChannelStatus, FriendMutation};
    use crate::state::{FunderMutation, FunderState};

    use crate::handler::state_wrap::{MutableEphemeral, MutableFunderState};
    use crate::handler::types::SendCommands;
    use crate::tests::utils::{dummy_named_relay_address, dummy_relay_address};

    #[test]
    fn test_handle_liveness_basic() {
        let rng1 = DummyRandom::new(&[1u8]);
        let private_key = PrivateKey::rand_gen(&rng1);
        let identity1 = SoftwareEd25519Identity::from_private_key(&private_key).unwrap();

        let rng2 = DummyRandom::new(&[2u8]);
        let private_key = PrivateKey::rand_gen(&rng2);
        let identity2 = SoftwareEd25519Identity::from_private_key(&private_key).unwrap();

        let pk1 = identity1.get_public_key();
        let pk2 = identity2.get_public_key();

        let (_local_identity, local_pk, _remote_identity, remote_pk) =
            if compare_public_key(&pk1, &pk2) == Ordering::Less {
                (identity1, pk1, identity2, pk2)
            } else {
                (identity2, pk2, identity1, pk1)
            };

        let relays = vec![dummy_named_relay_address(0)];
        let mut state = FunderState::<u32>::new(local_pk, relays);
        // Add a remote friend:
        let add_friend = AddFriend {
            friend_public_key: remote_pk.clone(),
            relays: vec![dummy_relay_address(1)],
            name: "remote_pk".into(),
        };
        let funder_mutation = FunderMutation::AddFriend(add_friend);
        state.mutate(&funder_mutation);

        // Enable the remote friend:
        let friend_mutation = FriendMutation::SetStatus(FriendStatus::Enabled);
        let funder_mutation = FunderMutation::FriendMutation((remote_pk.clone(), friend_mutation));
        state.mutate(&funder_mutation);

        // Make sure that our side of the token channel is outgoing:
        let friend = state.friends.get(&remote_pk).unwrap();
        let token_channel = match &friend.channel_status {
            ChannelStatus::Consistent(channel_consistent) => &channel_consistent.token_channel,
            _ => unreachable!(),
        };
        assert!(token_channel.get_outgoing().is_some());

        let ephemeral = Ephemeral::new();

        let mut m_state = MutableFunderState::new(state);
        let mut m_ephemeral = MutableEphemeral::new(ephemeral);
        let mut send_commands = SendCommands::new();
        let mut outgoing_control = Vec::new();
        let liveness_message = IncomingLivenessMessage::Online(remote_pk.clone());

        // Remote side got online:
        handle_liveness_message(
            &mut m_state,
            &mut m_ephemeral,
            &mut send_commands,
            &mut outgoing_control,
            &rng1,
            liveness_message,
        )
        .unwrap();

        let (_initial_state, funder_mutations, _final_state) = m_state.done();
        let (ephemeral_mutations, final_ephemeral_state) = m_ephemeral.done();

        assert!(outgoing_control.is_empty());
        assert!(funder_mutations.is_empty());
        assert_eq!(ephemeral_mutations.len(), 1);
        assert!(final_ephemeral_state.liveness.is_online(&remote_pk));

        // We expect that the local side will send the remote side a message:
        let friend_send_commands = send_commands.send_commands.get(&remote_pk).unwrap();
        assert!(friend_send_commands.resend_outgoing);
    }
}
