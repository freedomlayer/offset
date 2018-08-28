use futures::prelude::{async, await};
use ring::rand::SecureRandom;

use super::MutableFunderHandler;

pub enum HandleInitError {
}

#[allow(unused)]
impl<A: Clone + 'static, R: SecureRandom + 'static> MutableFunderHandler<A,R> {

    #[async]
    pub fn handle_init(mut self) -> Result<Self, !> {

        // TODO:
        // - Send all inconsistency error messages or outgoing move token messages
        //      (Don't send outgoing move token message if there is an inconsistency error
        //      message)
        // - Send all request token messages
        
        
        for (friend_public_key, friend) in self.state.get_friends() {
            // match friend.inconsistency_status.incoming {
            unimplemented!();

        }
        
        Ok(self)
    }

}
