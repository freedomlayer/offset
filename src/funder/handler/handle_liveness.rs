use futures::prelude::{async, await};
use ring::rand::SecureRandom;

use super::{MutableFunderHandler};
use super::super::types::{IncomingLivenessMessage};

pub enum HandleLivenessError {
}

#[allow(unused)]
impl<A: Clone + 'static, R: SecureRandom + 'static> MutableFunderHandler<A,R> {

    #[async]
    pub fn handle_liveness_message(mut self, 
                                  liveness_message: IncomingLivenessMessage) 
        -> Result<Self, !> {

        let fself = match liveness_message {
            IncomingLivenessMessage::Online(friend_public_key) => {
                self.ephemeral.liveness.set_online(&friend_public_key);
                self
            },
            IncomingLivenessMessage::Offline(friend_public_key) => {
                self.ephemeral.liveness.set_offline(&friend_public_key);
                // Cancel all messages pending for this friend.
                let fself = await!(self.cancel_pending_requests(
                        friend_public_key.clone()))?;
                let mut fself = await!(fself.cancel_pending_user_requests(
                        friend_public_key.clone()))?;
                fself
            },
        };
        Ok(fself)
    }

}
