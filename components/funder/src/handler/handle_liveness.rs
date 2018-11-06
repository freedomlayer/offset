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
                    ChannelStatus::Consistent(directional) => {
                        if directional.is_outgoing() {
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
