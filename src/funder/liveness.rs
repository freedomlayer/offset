use std::collections::hash_map::HashMap;
use crypto::identity::PublicKey;

const FRIEND_KEEPALIVE_TICKS: usize = 0x20;
const RETRANSMIT_TICKS: usize = 0x8;

pub enum LivenessError {
    FriendDoesNotExist,
    FriendAlreadyExists,
}

struct OnlineStatus {
    // Amount of ticks we leave to the remote side to send us anything.
    ticks_to_recv: usize,
    // When this value reaches 0, we retransmit outgoing message:
    ticks_to_retransmit_token_msg: Option<usize>,
    ticks_to_retransmit_inconsistency: Option<usize>,
}

enum LivenessStatus {
    Offline,
    Online(OnlineStatus),
}

pub struct FriendLiveness {
    // When this amount reaches 0, we send a keepalive to remote friend:
    ticks_to_send_keepalive: usize,
    liveness_status: LivenessStatus,
}

pub struct Liveness {
    pub friends: HashMap<PublicKey, FriendLiveness>,
}

#[derive(Clone, Copy)]
pub enum Direction {
    Incoming,
    Outgoing,
}

impl FriendLiveness {
    fn new(direction: Direction) -> FriendLiveness {
        let ticks_to_retransmit_token_msg = match direction {
            Direction::Incoming => None,
            Direction::Outgoing => Some(RETRANSMIT_TICKS),
        };
        FriendLiveness {
            ticks_to_send_keepalive: FRIEND_KEEPALIVE_TICKS/2,
            liveness_status: LivenessStatus::Online(OnlineStatus {
                ticks_to_recv: FRIEND_KEEPALIVE_TICKS,
                ticks_to_retransmit_token_msg,
                ticks_to_retransmit_inconsistency: None,
            })
        }
    }
    // TODO: Add different events for:
    // - Inconsistency messages (sent?)
    // - token messages sent
    // - acknowledged messages
    // - keepalive received

    fn time_tick(&mut self) {
    }

    /// A message was received from this remote friend
    fn message_received(&mut self) {
        self.liveness_status = LivenessStatus::Online(OnlineStatus {
            ticks_to_recv: FRIEND_KEEPALIVE_TICKS,
            ticks_to_retransmit_token_msg: None,
            ticks_to_retransmit_inconsistency: None,
        });
    }

    /// A message was sent to this remote friend
    fn message_sent(&mut self) {
        self.ticks_to_send_keepalive = FRIEND_KEEPALIVE_TICKS/2;
        if let LivenessStatus::Online(ref mut online_status) = self.liveness_status {
            online_status.ticks_to_retransmit_token_msg = Some(RETRANSMIT_TICKS);
            online_status.ticks_to_retransmit_inconsistency = None;
        }
    }
}


impl Liveness {
    fn new() -> Liveness {
        Liveness {
            friends: HashMap::new(),
        }
    }

    fn insert_friend(&mut self, friend_public_key: &PublicKey, direction: Direction) -> Result<(), LivenessError> {
        if self.friends.contains_key(friend_public_key) {
            Err(LivenessError::FriendAlreadyExists)
        } else {
            self.friends.insert(friend_public_key.clone(), FriendLiveness::new(direction));
            Ok(())
        }
    }

    fn remove_friend(&mut self, friend_public_key: &PublicKey) -> Result<(), LivenessError> {
        self.friends.remove(&friend_public_key).ok_or(LivenessError::FriendDoesNotExist)?;
        Ok(())
    }

    fn time_tick(&mut self) {
    }
}
