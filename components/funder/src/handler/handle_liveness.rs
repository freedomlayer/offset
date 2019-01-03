use std::fmt::Debug;
use crypto::crypto_rand::CryptoRandom;

use common::canonical_serialize::CanonicalSerialize;
use proto::funder::messages::{FriendMessage, FriendStatus};

use crate::handler::MutableFunderHandler;
use crate::handler::sender::SendMode;
use crate::friend::ChannelStatus;
use crate::types::{IncomingLivenessMessage, 
    FunderOutgoingComm};

use crate::ephemeral::EphemeralMutation;
use crate::liveness::LivenessMutation;

#[derive(Debug)]
pub enum HandleLivenessError {
    FriendDoesNotExist,
    FriendIsDisabled,
    FriendAlreadyOnline,
}

#[allow(unused)]
impl<A,R> MutableFunderHandler<A,R> 
where
    A: CanonicalSerialize + Clone + Debug + PartialEq + Eq + 'static,
    R: CryptoRandom + 'static,
{

    pub async fn handle_liveness_message(&mut self, 
                                  liveness_message: IncomingLivenessMessage) 
        -> Result<(), HandleLivenessError> {

        match liveness_message {
            IncomingLivenessMessage::Online(friend_public_key) => {
                // Find friend:
                let friend = match self.get_friend(&friend_public_key) {
                    Some(friend) => Ok(friend),
                    None => Err(HandleLivenessError::FriendDoesNotExist),
                }?;
                match friend.status {
                    FriendStatus::Enabled => Ok(()),
                    FriendStatus::Disabled => Err(HandleLivenessError::FriendIsDisabled),
                }?;

                if self.ephemeral.liveness.is_online(&friend_public_key) {
                    return Err(HandleLivenessError::FriendAlreadyOnline);
                }

                match &friend.channel_status {
                    ChannelStatus::Consistent(token_channel) => {
                        if token_channel.is_outgoing() {
                            self.transmit_outgoing(&friend_public_key);
                        }
                        await!(self.try_send_channel(&friend_public_key, SendMode::EmptyNotAllowed));
                    },
                    ChannelStatus::Inconsistent(channel_inconsistent) => {
                        self.add_outgoing_comm(
                            FunderOutgoingComm::FriendMessage((friend_public_key.clone(),
                                FriendMessage::InconsistencyError(channel_inconsistent.local_reset_terms.clone()))));
                    },
                };

                let liveness_mutation = LivenessMutation::SetOnline(friend_public_key.clone());
                let ephemeral_mutation = EphemeralMutation::LivenessMutation(liveness_mutation);
                self.apply_ephemeral_mutation(ephemeral_mutation);
            },
            IncomingLivenessMessage::Offline(friend_public_key) => {
                // Find friend:
                let friend = match self.get_friend(&friend_public_key) {
                    Some(friend) => Ok(friend),
                    None => Err(HandleLivenessError::FriendDoesNotExist),
                }?;
                match friend.status {
                    FriendStatus::Enabled => Ok(()),
                    FriendStatus::Disabled => Err(HandleLivenessError::FriendIsDisabled),
                }?;
                let liveness_mutation = LivenessMutation::SetOffline(friend_public_key.clone());
                let ephemeral_mutation = EphemeralMutation::LivenessMutation(liveness_mutation);
                self.apply_ephemeral_mutation(ephemeral_mutation);

                // Cancel all messages pending for this friend.
                await!(self.cancel_pending_requests(
                        friend_public_key.clone()));
                await!(self.cancel_pending_user_requests(
                        friend_public_key.clone()));
            },
        };
        Ok(())
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    use std::cmp::Ordering;

    use proto::funder::messages::{FriendStatus, AddFriend};

    use crate::handler::gen_mutable;
    use crate::state::{FunderState, FunderMutation};
    use crate::ephemeral::Ephemeral;
    use crate::token_channel::TcDirection;
    use crate::friend::FriendMutation;

    use futures::executor::ThreadPool;
    use futures::{future, FutureExt};
    use futures::task::SpawnExt;
    use identity::{create_identity, IdentityClient};

    use crypto::test_utils::DummyRandom;
    use crypto::identity::{SoftwareEd25519Identity,
                            generate_pkcs8_key_pair, compare_public_key};
    use crypto::crypto_rand::RngContainer;


    async fn task_handle_liveness_basic(identity_client1: IdentityClient, 
                                        identity_client2: IdentityClient) {

        let pk1 = await!(identity_client1.request_public_key()).unwrap();
        let pk2 = await!(identity_client2.request_public_key()).unwrap();

        let (local_identity, local_pk, _remote_identity, remote_pk) = if compare_public_key(&pk1, &pk2) == Ordering::Less {
            (identity_client1, pk1, identity_client2, pk2)
        } else {
            (identity_client2, pk2, identity_client1, pk1)
        };

        let mut state = FunderState::new(&local_pk, Some(&1337u32));
        // Add a remote friend:
        let add_friend = AddFriend {
            friend_public_key: remote_pk.clone(),
            address: 3u32,
            name: "remote_pk".into(),
            balance: 0i128,
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
            ChannelStatus::Consistent(token_channel) => token_channel,
            _ => unreachable!(),
        };
        assert!(token_channel.is_outgoing());

        let move_token_out = match token_channel.get_direction() {
            TcDirection::Outgoing(tc_outgoing) => tc_outgoing.move_token_out.clone(),
            _ => unreachable!(),
        };

        let ephemeral = Ephemeral::new(&state);
        let rng = DummyRandom::new(&[2u8]);

        let mut mutable_funder_handler = gen_mutable(local_identity,
                    RngContainer::new(rng),
                    &state,
                    &ephemeral);

        // Remote side got online:
        await!(mutable_funder_handler.handle_liveness_message(
            IncomingLivenessMessage::Online(remote_pk.clone()))).unwrap();

        // We expect that the local side will send the remote side a message:
        let mut funder_handler_output = mutable_funder_handler.done();
        assert!(funder_handler_output.funder_mutations.is_empty());
        assert_eq!(funder_handler_output.outgoing_control.len(), 1);
        assert_eq!(funder_handler_output.outgoing_comms.len(),1);
        let out_comm = funder_handler_output.outgoing_comms.pop().unwrap();

        let (public_key, friend_message) = match out_comm {
            FunderOutgoingComm::FriendMessage(x) => x,
            _ => unreachable!(),
        };

        assert_eq!(public_key, remote_pk);

        let friend_move_token_request = match friend_message {
            FriendMessage::MoveTokenRequest(friend_move_token_request) => friend_move_token_request,
            _ => unreachable!(),
        };

        assert!(!friend_move_token_request.token_wanted);
        assert_eq!(&friend_move_token_request.friend_move_token, &move_token_out);
    }

    #[test]
    fn test_handle_liveness_basic() {
        // Start identity service:
        let mut thread_pool = ThreadPool::new().unwrap();

        let rng1 = DummyRandom::new(&[1u8]);
        let pkcs8 = generate_pkcs8_key_pair(&rng1);
        let identity1 = SoftwareEd25519Identity::from_pkcs8(&pkcs8).unwrap();
        let (requests_sender1, identity_server1) = create_identity(identity1);
        let identity_client1 = IdentityClient::new(requests_sender1);
        thread_pool.spawn(identity_server1.then(|_| future::ready(()))).unwrap();

        let rng2 = DummyRandom::new(&[2u8]);
        let pkcs8 = generate_pkcs8_key_pair(&rng2);
        let identity2 = SoftwareEd25519Identity::from_pkcs8(&pkcs8).unwrap();
        let (requests_sender2, identity_server2) = create_identity(identity2);
        let identity_client2 = IdentityClient::new(requests_sender2);
        thread_pool.spawn(identity_server2.then(|_| future::ready(()))).unwrap();

        thread_pool.run(task_handle_liveness_basic(identity_client1, identity_client2));
    }
}
