use crypto::crypto_rand::CryptoRandom;

use super::MutableFunderHandler;
use super::super::token_channel::directional::MoveTokenDirection;
use super::super::types::{FriendMessage,
                            FriendInconsistencyError, ChannelerConfig,
                            FriendStatus, ResetTerms,
                            FunderOutgoingComm};

pub enum HandleInitError {
}

#[allow(unused)]
impl<A: Clone, R: CryptoRandom> MutableFunderHandler<A,R> {

    pub fn handle_init(&mut self) {
        let mut enabled_friends = Vec::new();
        for (friend_public_key, friend) in self.state.get_friends() {
            match friend.status {
                FriendStatus::Enable => {
                    enabled_friends.push((friend.remote_public_key.clone(),
                        friend.remote_address.clone()));
                },
                FriendStatus::Disable => continue,
            };
        }

        for enabled_friend in enabled_friends {
            // Notify Channeler:
            let channeler_config = ChannelerConfig::AddFriend(enabled_friend);
            self.add_outgoing_comm(FunderOutgoingComm::ChannelerConfig(channeler_config));
        }
    }
}
