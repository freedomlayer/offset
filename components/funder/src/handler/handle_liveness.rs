use crypto::crypto_rand::CryptoRandom;

use super::MutableFunderHandler;
use super::super::friend::ChannelStatus;
use super::super::types::{IncomingLivenessMessage, FriendStatus, 
    FriendMessage, FunderOutgoingComm};

#[derive(Debug)]
pub enum HandleLivenessError {
    FriendDoesNotExist,
    FriendIsDisabled,
    FriendAlreadyOnline,
}

#[allow(unused)]
impl<A: Clone + 'static, R: CryptoRandom + 'static> MutableFunderHandler<A,R> {

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
                    FriendStatus::Enable => Ok(()),
                    FriendStatus::Disable => Err(HandleLivenessError::FriendIsDisabled),
                }?;

                if self.ephemeral.liveness.is_online(&friend_public_key) {
                    return Err(HandleLivenessError::FriendAlreadyOnline);
                }

                match &friend.channel_status {
                    ChannelStatus::Consistent(token_channel) => {
                        if token_channel.is_outgoing() {
                            self.transmit_outgoing(&friend_public_key);
                        }
                    },
                    ChannelStatus::Inconsistent(channel_inconsistent) => {
                        self.add_outgoing_comm(
                            FunderOutgoingComm::FriendMessage((friend_public_key.clone(),
                                FriendMessage::InconsistencyError(channel_inconsistent.local_reset_terms.clone()))));
                    },
                };

                self.ephemeral.liveness.set_online(&friend_public_key);
            },
            IncomingLivenessMessage::Offline(friend_public_key) => {
                // Find friend:
                let friend = match self.get_friend(&friend_public_key) {
                    Some(friend) => Ok(friend),
                    None => Err(HandleLivenessError::FriendDoesNotExist),
                }?;
                match friend.status {
                    FriendStatus::Enable => Ok(()),
                    FriendStatus::Disable => Err(HandleLivenessError::FriendIsDisabled),
                }?;
                self.ephemeral.liveness.set_offline(&friend_public_key);
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

    use std::rc::Rc;

    use crate::handler::gen_mutable;
    use crate::state::{FunderState, FunderMutation};
    use crate::ephemeral::FunderEphemeral;
    use crate::token_channel::is_public_key_lower;
    use crate::types::{ChannelerConfig, FriendStatus};
    use crate::friend::FriendMutation;

    use futures::executor::ThreadPool;
    use futures::{future, FutureExt};
    use futures::task::SpawnExt;
    use identity::{create_identity, IdentityClient};

    use crypto::test_utils::DummyRandom;
    use crypto::identity::{SoftwareEd25519Identity,
                            generate_pkcs8_key_pair, PUBLIC_KEY_LEN,
                            PublicKey};


    async fn task_handle_liveness_basic(identity_client1: IdentityClient, 
                                        identity_client2: IdentityClient) {

        let pk1 = await!(identity_client1.request_public_key()).unwrap();
        let pk2 = await!(identity_client2.request_public_key()).unwrap();

        let (local_identity, local_pk, remote_identity, remote_pk) = if is_public_key_lower(&pk1, &pk2) {
            (identity_client1, pk1, identity_client2, pk2)
        } else {
            (identity_client2, pk2, identity_client1, pk1)
        };

        let mut state = FunderState::new(&local_pk);
        // Add a remote friend:
        let funder_mutation = FunderMutation::AddFriend((remote_pk.clone(), 3u32)); // second arg is address
        state.mutate(&funder_mutation);

        // Enable the remote friend:
        let friend_mutation = FriendMutation::SetStatus(FriendStatus::Enable);
        let funder_mutation = FunderMutation::FriendMutation((remote_pk.clone(), friend_mutation));
        state.mutate(&funder_mutation);

        // Make sure that our side of the token channel is outgoing:
        let friend = state.friends.get(&remote_pk).unwrap();
        let token_channel = match &friend.channel_status {
            ChannelStatus::Consistent(token_channel) => token_channel,
            _ => unreachable!(),
        };
        assert!(token_channel.is_outgoing());

        let ephemeral = FunderEphemeral::new(&state);
        let rng = DummyRandom::new(&[2u8]);

        let mut mutable_funder_handler = gen_mutable(local_identity,
                    Rc::new(rng),
                    &state,
                    &ephemeral);

        // Remote side got online:
        await!(mutable_funder_handler.handle_liveness_message(
            IncomingLivenessMessage::Online(remote_pk.clone()))).unwrap();

        // We expect that the local side will send the remote side a message:
        let mut funder_handler_output = mutable_funder_handler.done();
        assert!(funder_handler_output.mutations.is_empty());
        assert!(funder_handler_output.outgoing_control.is_empty());
        assert_eq!(funder_handler_output.outgoing_comms.len(),1);
        let out_comm = funder_handler_output.outgoing_comms.pop().unwrap();

        /*
        let channeler_config = match out_comm {
            FunderOutgoingComm::ChannelerConfig(channeler_config) => channeler_config,
            _ => unreachable!(),
        };
        match channeler_config {
            ChannelerConfig::AddFriend(friend_tuple) => {
                assert_eq!(friend_tuple, (remote_pk, 3u32));
            },
            _ => unreachable!(),
        };
        */
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
