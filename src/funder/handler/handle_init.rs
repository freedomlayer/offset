use futures::prelude::{async, await};
use ring::rand::SecureRandom;

use super::MutableFunderHandler;
use super::super::token_channel::directional::MoveTokenDirection;
use super::super::types::{FunderTask, FriendMessage,
                            FriendInconsistencyError, ChannelerConfig,
                            FriendStatus, ResetTerms};

pub enum HandleInitError {
}

#[allow(unused)]
impl<A: Clone, R: SecureRandom> MutableFunderHandler<A,R> {

    pub fn handle_init(&mut self) {
        // let mut retransmit_pks = Vec::new();
        // let mut inconsistents = Vec::new();

        let mut enabled_friends = Vec::new();
        for (friend_public_key, friend) in self.state.get_friends() {
            match friend.status {
                FriendStatus::Enable => {
                    enabled_friends.push((friend.remote_public_key.clone(),
                        friend.opt_remote_address.clone()));
                },
                FriendStatus::Disable => continue,
            };

            /*
            // match friend.inconsistency_status.incoming {
            match &friend.inconsistency_status {
                InconsistencyStatus::Empty => {
                    match &friend.directional.direction {
                        MoveTokenDirection::Outgoing(_) => {
                            retransmit_pks.push(friend_public_key.clone());
                        },
                        MoveTokenDirection::Incoming(_) => {}
                    }
                },
                InconsistencyStatus::Outgoing(out_reset_terms) => {
                    inconsistents.push((friend_public_key.clone(),
                        out_reset_terms.clone()))
                },
                InconsistencyStatus::IncomingOutgoing((_in_reset_terms, out_reset_terms)) => {
                    inconsistents.push((friend_public_key.clone(),
                        out_reset_terms.clone()))
                },
            };
            */
        }

        for enabled_friend in enabled_friends {
            // Notify Channeler:
            let channeler_config = ChannelerConfig::AddFriend(enabled_friend);
            self.funder_tasks.push(FunderTask::ChannelerConfig(channeler_config));
        }


        /*
        for friend_public_key in retransmit_pks {
            self.transmit_outgoing(&friend_public_key);
        }

        for (friend_public_key, out_reset_terms) in inconsistents {
            let ResetTerms {reset_token, balance_for_reset} = out_reset_terms;
            let inconsistency_error = FriendInconsistencyError {
                reset_token,
                balance_for_reset,
            };
            self.funder_tasks.push(
                FunderTask::FriendMessage((friend_public_key,
                    FriendMessage::InconsistencyError(inconsistency_error))));
        }
        */
    }
}
