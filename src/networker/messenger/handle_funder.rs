use ring::rand::SecureRandom;
use super::messenger_handler::{MessengerHandler, MessengerTask};

#[allow(unused)]
impl<R: SecureRandom> MessengerHandler<R> {
    pub fn handle_funder_message(&mut self) -> Vec<MessengerTask> {
        // TODO
        unreachable!();
    }
}
