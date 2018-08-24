use std::collections::hash_map::HashMap;
use im::hashmap::HashMap as ImHashMap;
use crypto::identity::PublicKey;

const FRIEND_KEEPALIVE_TICKS: usize = 0x20;
const RETRANSMIT_TICKS: usize = 0x8;

pub enum LivenessError {
    FriendDoesNotExist,
    FriendAlreadyExists,
}


#[derive(Clone)]
pub enum LivenessStatus {
    Offline,
    Online(usize),  // When reaches 0, friend is considered to be offline
}

#[derive(Clone)]
pub struct FriendLiveness {
    liveness_status: LivenessStatus,
    ticks_retransmit_inconsistency: Option<usize>,
    ticks_retransmit_token_msg: Option<usize>,
    // When this amount reaches 0, we send a keepalive to remote friend:
    ticks_send_keepalive: usize,
}

pub struct Actions {
    retransmit_inconsistency: bool,
    retransmit_token_msg: bool,
    send_keepalive: bool,
}

impl Actions {
    fn new() -> Actions {
        Actions {
            retransmit_inconsistency: false,
            retransmit_token_msg: false,
            send_keepalive: false,
        }
    }
}

#[derive(Clone)]
pub struct Liveness {
    pub friends: ImHashMap<PublicKey, FriendLiveness>,
}


#[derive(Clone, Copy)]
pub enum Direction {
    Incoming,
    Outgoing(bool), // is_acked
}

impl FriendLiveness {
    fn new(direction: Direction) -> FriendLiveness {
        let ticks_retransmit_token_msg = match direction {
            Direction::Incoming => None,
            Direction::Outgoing(false) => Some(RETRANSMIT_TICKS),
            Direction::Outgoing(true) => None,
        };
        FriendLiveness {
            liveness_status: LivenessStatus::Offline,
            ticks_retransmit_inconsistency: None,
            ticks_retransmit_token_msg,
            ticks_send_keepalive: FRIEND_KEEPALIVE_TICKS/2,
        }
    }

    fn message_sent(&mut self) {
        self.ticks_send_keepalive = FRIEND_KEEPALIVE_TICKS/2;
    }

    fn set_friend_online(&mut self) -> Actions {
        let mut actions = Actions::new();
        match &self.liveness_status {
            LivenessStatus::Online(ticks_to_offline) => {
                self.liveness_status = LivenessStatus::Online(FRIEND_KEEPALIVE_TICKS);
            },
            LivenessStatus::Offline => {
                self.ticks_send_keepalive = FRIEND_KEEPALIVE_TICKS;
                actions.send_keepalive = true;
                self.liveness_status = LivenessStatus::Online(FRIEND_KEEPALIVE_TICKS);
                if self.ticks_retransmit_inconsistency.is_some() {
                    self.ticks_retransmit_inconsistency = Some(RETRANSMIT_TICKS);
                    actions.retransmit_inconsistency = true;
                    actions.send_keepalive = false;
                }
                if self.ticks_retransmit_token_msg.is_some() {
                    self.ticks_retransmit_token_msg = Some(RETRANSMIT_TICKS);
                    actions.retransmit_token_msg = true;
                    actions.send_keepalive = false;
                }
            },
        };
        actions
    }

    /// A message was received from this remote friend
    fn move_token_received(&mut self) -> Actions {
        self.ticks_retransmit_token_msg = None;
        self.ticks_retransmit_inconsistency = None;
        self.set_friend_online()
    }

    /// A message was sent to this remote friend
    fn move_token_sent(&mut self) -> Actions{
        self.message_sent();
        self.ticks_retransmit_token_msg = Some(RETRANSMIT_TICKS);
        self.ticks_retransmit_inconsistency = None;
        Actions::new()
    }

    fn move_token_ack_received(&mut self) -> Actions {
        self.ticks_retransmit_token_msg = None;
        // TODO: Should we clear ticks_retransmit_inconsistency here?
        self.set_friend_online()
    }

    fn move_token_ack_sent(&mut self) -> Actions {
        self.message_sent();
        Actions::new()
    }

    fn keepalive_received(&mut self) -> Actions {
        self.set_friend_online()
    }

    fn keepalive_sent(&mut self) -> Actions {
        self.message_sent();
        Actions::new()
    }

    fn inconsistency_received(&mut self) -> Actions {
        self.set_friend_online()
    }

    fn inconsistency_sent(&mut self) -> Actions {
        self.message_sent();
        self.ticks_retransmit_inconsistency = Some(RETRANSMIT_TICKS);
        Actions::new()
    }

    fn time_tick(&mut self) -> Actions {
        let mut actions = Actions::new();
        self.ticks_send_keepalive = self.ticks_send_keepalive.checked_sub(1).unwrap();
        if self.ticks_send_keepalive == 0 {
            self.ticks_send_keepalive = FRIEND_KEEPALIVE_TICKS;
            actions.send_keepalive = true;
        }
        self.liveness_status = match self.liveness_status {
            LivenessStatus::Offline => LivenessStatus::Offline,
            LivenessStatus::Online(ref mut ticks_to_offline) => {
                *ticks_to_offline = ticks_to_offline.checked_sub(1).unwrap();
                if *ticks_to_offline == 0 {
                    LivenessStatus::Offline
                } else {
                    LivenessStatus::Online(*ticks_to_offline)
                }
            }
        };

        // In case we still think that the friend is online, we go on to decrement the rest of the
        // time ticks:
        if let LivenessStatus::Online(_) = self.liveness_status {
            self.ticks_retransmit_inconsistency = match self.ticks_retransmit_inconsistency {
                Some(ref mut retransmit_ticks) => {
                    *retransmit_ticks = retransmit_ticks.checked_sub(1).unwrap();
                    if *retransmit_ticks == 0 {
                        *retransmit_ticks = RETRANSMIT_TICKS;
                        actions.retransmit_inconsistency = true;
                        actions.send_keepalive = false;
                    }
                    Some(*retransmit_ticks)
                },
                None => None,
            };
            self.ticks_retransmit_token_msg = match self.ticks_retransmit_token_msg {
                Some(ref mut retransmit_ticks) => {
                    *retransmit_ticks = retransmit_ticks.checked_sub(1).unwrap();
                    if *retransmit_ticks == 0 {
                        *retransmit_ticks = RETRANSMIT_TICKS;
                        actions.retransmit_token_msg = true;
                        actions.send_keepalive = false;
                    }
                    Some(*retransmit_ticks)
                },
                None => None,
            };
        }
        actions
    }

}


impl Liveness {
    fn new() -> Liveness {
        Liveness {
            friends: ImHashMap::new(),
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
