use ring::rand::SecureRandom;
use super::messenger_handler::{MessengerHandler, MessengerTask};

#[allow(unused)]
impl<R: SecureRandom> MessengerHandler<R> {
    pub fn handle_crypter(&mut self) -> Vec<MessengerTask> {
        // TODO
        unreachable!();
    }
}

