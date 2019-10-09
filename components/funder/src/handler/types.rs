use std::collections::HashMap;

use proto::crypto::PublicKey;

#[derive(Debug, Clone)]
pub struct FriendSendCommands {
    /// Try to send whatever possible through this friend.
    pub try_send: bool,
    /// Resend the outgoing move token message
    pub resend_outgoing: bool,
    /// Remote friend wants the token.
    pub remote_wants_token: bool,
    /// We want to perform a local reset
    pub local_reset: bool,
}

impl FriendSendCommands {
    fn new() -> Self {
        FriendSendCommands {
            try_send: false,
            resend_outgoing: false,
            remote_wants_token: false,
            local_reset: false,
        }
    }
}

#[derive(Clone)]
pub struct SendCommands {
    pub send_commands: HashMap<PublicKey, FriendSendCommands>,
}

impl SendCommands {
    pub fn new() -> Self {
        SendCommands {
            send_commands: HashMap::new(),
        }
    }

    pub fn set_try_send(&mut self, friend_public_key: &PublicKey) {
        let friend_send_commands = self
            .send_commands
            .entry(friend_public_key.clone())
            .or_insert_with(FriendSendCommands::new);
        friend_send_commands.try_send = true;
    }

    pub fn set_resend_outgoing(&mut self, friend_public_key: &PublicKey) {
        let friend_send_commands = self
            .send_commands
            .entry(friend_public_key.clone())
            .or_insert_with(FriendSendCommands::new);
        friend_send_commands.resend_outgoing = true;
    }

    pub fn set_remote_wants_token(&mut self, friend_public_key: &PublicKey) {
        let friend_send_commands = self
            .send_commands
            .entry(friend_public_key.clone())
            .or_insert_with(FriendSendCommands::new);
        friend_send_commands.remote_wants_token = true;
    }

    pub fn set_local_reset(&mut self, friend_public_key: &PublicKey) {
        let friend_send_commands = self
            .send_commands
            .entry(friend_public_key.clone())
            .or_insert_with(FriendSendCommands::new);
        friend_send_commands.local_reset = true;
    }
}
