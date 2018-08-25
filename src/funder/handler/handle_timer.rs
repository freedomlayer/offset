use ring::rand::SecureRandom;

use crypto::identity::PublicKey;

use super::{FriendInconsistencyError, FunderTask, FriendMessage};
use super::super::liveness::Actions;
use super::super::friend::{IncomingInconsistency, OutgoingInconsistency};

use super::MutableFunderHandler;

impl<A:Clone, R:SecureRandom> MutableFunderHandler<A,R> {

    fn invoke_actions(&mut self, 
                      remote_public_key: &PublicKey,
                      actions: &Actions) {

        if actions.retransmit_inconsistency {
            let friend = self.get_friend(&remote_public_key).unwrap();
            // Check if we have an inconsistency message to ack:
            let opt_ack = match &friend.inconsistency_status.incoming {
                IncomingInconsistency::Empty => None,
                IncomingInconsistency::Incoming(reset_terms) => Some(reset_terms.current_token.clone()),
            };

            let reset_terms = match &friend.inconsistency_status.outgoing {
                OutgoingInconsistency::Empty | OutgoingInconsistency::Acked => unreachable!(),
                OutgoingInconsistency::Sent(reset_terms) => reset_terms
            };

            let inconsistency_error = FriendInconsistencyError {
                opt_ack,
                current_token: reset_terms.current_token.clone(),
                balance_for_reset: reset_terms.balance_for_reset,
            };

            self.add_task(
                FunderTask::FriendMessage(
                    FriendMessage::InconsistencyError(inconsistency_error)));
        }

        if actions.retransmit_token_msg {
            let friend = self.get_friend(&remote_public_key).unwrap();
            let outgoing_move_token = friend.directional.get_outgoing_move_token().unwrap();
            // Add a task for sending the outgoing move token:
            self.add_task(
                FunderTask::FriendMessage(
                    FriendMessage::MoveToken(outgoing_move_token)));
        }

        if actions.send_keepalive {
            self.add_task(
                FunderTask::FriendMessage(
                    FriendMessage::KeepAlive));
        }
    }

    fn handle_timer_tick(&mut self) {
        let time_tick_output = self.ephemeral.liveness.time_tick();
        for (friend_public_key, actions) in &time_tick_output.friends_actions {
            self.invoke_actions(friend_public_key, actions);
        }

        // TODO:
        // - For any friend that just got offline:
        //      - Cancel all pending requests.

        unimplemented!();
    }
}
