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
    pub retransmit_inconsistency: bool,
    pub retransmit_token_msg: bool,
    pub send_keepalive: bool,
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

    pub fn is_online(&self) -> bool {
        match self.liveness_status {
            LivenessStatus::Online(_) => true,
            LivenessStatus::Offline => false,
        }
    }

    pub fn message_sent(&mut self) {
        self.ticks_send_keepalive = FRIEND_KEEPALIVE_TICKS/2;
    }

    pub fn message_received(&mut self) {
        match &self.liveness_status {
            LivenessStatus::Online(ticks_to_offline) => {
                self.liveness_status = LivenessStatus::Online(FRIEND_KEEPALIVE_TICKS);
            },
            LivenessStatus::Offline => {
                self.ticks_send_keepalive = FRIEND_KEEPALIVE_TICKS;
                self.liveness_status = LivenessStatus::Online(FRIEND_KEEPALIVE_TICKS);
                if self.ticks_retransmit_inconsistency.is_some() {
                    // Will be triggered in the next tick:
                    self.ticks_retransmit_inconsistency = Some(1); 
                }
                if self.ticks_retransmit_token_msg.is_some() {
                    // Will be triggered in the next tick:
                    self.ticks_retransmit_token_msg = Some(1);
                }
            },
        };
    }

    pub fn cancel_inconsistency(&mut self) {
        self.ticks_retransmit_inconsistency = None;
    }

    pub fn reset_inconsistency(&mut self) {
        self.ticks_retransmit_inconsistency = Some(RETRANSMIT_TICKS);
    }

    pub fn cancel_token_msg(&mut self) {
        self.ticks_retransmit_token_msg = None;
    }

    pub fn reset_token_msg(&mut self) {
        self.ticks_retransmit_token_msg = Some(RETRANSMIT_TICKS);
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

pub struct TimeTickOutput {
    pub became_offline: Vec<PublicKey>,
    pub friends_actions: Vec<(PublicKey, Actions)>,
}

impl TimeTickOutput {
    fn new() -> TimeTickOutput {
        TimeTickOutput {
            became_offline: Vec::new(),
            friends_actions: Vec::new(),
        }
    }
}


impl Liveness {
    pub fn new() -> Liveness {
        Liveness {
            friends: ImHashMap::new(),
        }
    }

    pub fn add_friend(&mut self, friend_public_key: &PublicKey, direction: Direction) -> Result<(), LivenessError> {
        if self.friends.contains_key(friend_public_key) {
            Err(LivenessError::FriendAlreadyExists)
        } else {
            self.friends.insert(friend_public_key.clone(), FriendLiveness::new(direction));
            Ok(())
        }
    }

    pub fn remove_friend(&mut self, friend_public_key: &PublicKey) -> Result<(), LivenessError> {
        self.friends.remove(&friend_public_key).ok_or(LivenessError::FriendDoesNotExist)?;
        Ok(())
    }

    pub fn time_tick(&mut self) -> TimeTickOutput {
        let mut friend_public_keys = self.friends.keys().cloned().collect::<Vec<PublicKey>>();
        friend_public_keys.sort_unstable();
        let friend_public_keys = friend_public_keys;

        let mut time_tick_output = TimeTickOutput::new();

        for friend_public_key in &friend_public_keys {
            let mut friend = self.friends.get_mut(friend_public_key).unwrap();
            let is_online_before = friend.is_online();
            let actions = friend.time_tick();
            time_tick_output.friends_actions.push((friend_public_key.clone(), actions));
            let is_online_after = friend.is_online();

            // We can detect if the remote friend has became offline:
            if is_online_before && !is_online_after {
                time_tick_output.became_offline.push(friend_public_key.clone());
            }
        }
        time_tick_output
    }
}


// TODO: Add tests.
