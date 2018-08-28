use futures::prelude::{async, await};
use ring::rand::SecureRandom;

use super::{MutableFunderHandler, FriendInconsistencyError, 
    FriendMessage, FunderTask};
use super::super::friend::{InconsistencyStatus, ResetTerms};
use super::super::token_channel::directional::MoveTokenDirection;

pub enum HandleInitError {
}

#[allow(unused)]
impl<A: Clone + 'static, R: SecureRandom + 'static> MutableFunderHandler<A,R> {

    pub fn handle_init(&mut self) {
        let mut retransmit_pks = Vec::new();
        let mut inconsistents = Vec::new();
        let mut request_tokens = Vec::new();

        for (friend_public_key, friend) in self.state.get_friends() {
            // match friend.inconsistency_status.incoming {
            match &friend.inconsistency_status {
                InconsistencyStatus::Empty => {
                    match &friend.directional.direction {
                        MoveTokenDirection::Outgoing(outgoing_move_token) => {
                            retransmit_pks.push(friend_public_key.clone());
                            if outgoing_move_token.request_token_sent {
                                request_tokens.push((
                                        friend_public_key.clone(),
                                        friend.directional.new_token()));
                            }
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
        }

        for friend_public_key in retransmit_pks {
            self.transmit_outgoing(&friend_public_key);
        }

        for (friend_public_key, new_token) in request_tokens {
            // TODO: What if the request_token message arrives out of order?
            // Generally, how should we solve this problem?
            self.funder_tasks.push(
                FunderTask::FriendMessage((friend_public_key,
                        FriendMessage::RequestToken(new_token))));
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
    }
}
