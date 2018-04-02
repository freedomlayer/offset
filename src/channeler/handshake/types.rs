use std::collections::{HashMap, HashSet};
use std::convert::TryFrom;

use crypto::dh::{DhPrivateKey, DhPublicKey, Salt};
use crypto::hash::{sha_512_256, HashResult};
use crypto::identity::PublicKey;
use crypto::rand_values::RandValue;
use crypto::sym_encrypt::SymmetricKey;
use crypto::CryptoError;
use proto::channeler::{ChannelId, CHANNEL_ID_LEN};

use ring::aead::{CHACHA20_POLY1305, SealingKey, OpeningKey};

use super::state_machine::HandshakeState;

pub struct NewChannelInfo {
    pub sender_id: ChannelId,
    pub sender_key: SealingKey,
    pub receiver_id: ChannelId,
    pub receiver_key: OpeningKey,
    pub remote_public_key: PublicKey,
}

#[derive(Debug)]
pub enum HandshakeError {
    AlreadyExist,

    NotAllowed,

    NoSuchSession,

    InvalidKey,

    InvalidSignature,

    Incomplete,

    InvalidTransfer,

    CryptoError(CryptoError),
}

#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub enum HandshakeRole {
    Initiator,
    Responder,
}

/// The identifier of a handshake session.
#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub struct HandshakeId(HandshakeRole, PublicKey);

impl HandshakeId {
    pub fn new(role: HandshakeRole, remote_public_key: PublicKey) -> HandshakeId {
        HandshakeId(role, remote_public_key)
    }

    pub fn is_initiator(&self) -> bool {
        match self.0 {
            HandshakeRole::Initiator => true,
            HandshakeRole::Responder => false,
        }
    }

    pub fn remote_public_key(&self) -> &PublicKey {
        &self.1
    }
}

pub struct HandshakeSession {
    pub id: HandshakeId,
    pub state: HandshakeState,
    timeout_ticks: usize,
}

pub struct HandshakeSessionMap {
    in_flight_ids: HashSet<HandshakeId>,
    last_hash_map: HashMap<HashResult, HandshakeSession>,
}

impl HandshakeSession {
    /// Creates a new `HandshakeSession`.
    pub fn new(id: HandshakeId, state: HandshakeState, timeout: usize) -> HandshakeSession {
        HandshakeSession {
            id,
            state,
            timeout_ticks: timeout,
        }
    }
    /// Returns a reference of remote's public key.
    pub fn remote_public_key(&self) -> &PublicKey {
        self.id.remote_public_key()
    }

    /// Whether this handshake session initiated by local.
    pub fn is_initiator(&self) -> bool {
        self.id.is_initiator()
    }

    // Finish handshake session and returns the `HandshakeResult` on success.
    //
    // # Panics
    //
    // Panics if the state of the session isn't `HandshakeState::ExchangePassive`
    pub fn finish(self) -> Result<NewChannelInfo, HandshakeError> {
        let is_initiator = self.is_initiator();
        let remote_public_key = self.remote_public_key().clone();

        match self.state {
            HandshakeState::ExchangePassive {
                initiator_rand_nonce,
                responder_rand_nonce,
                remote_dh_public_key,
                initiator_key_salt,
                responder_key_salt,
                my_private_key,
            } => {
                let (initiator_dh_public_key, responder_dh_public_key) = {
                    let my_dh_public_key = my_private_key.compute_public_key()
                        .map_err(HandshakeError::CryptoError)?;

                    if is_initiator {
                        (my_dh_public_key, remote_dh_public_key)
                    } else {
                        (remote_dh_public_key, my_dh_public_key)
                    }
                };

                let (initiator_id, responder_id) = derive_channel_id(
                    &initiator_rand_nonce,
                    &responder_rand_nonce,
                    &initiator_dh_public_key,
                    &responder_dh_public_key,
                );

                let sender_id: ChannelId;
                let receiver_id: ChannelId;
                let sender_key: SealingKey;
                let receiver_key: OpeningKey;

                if is_initiator {
                    sender_id = initiator_id;
                    receiver_id = responder_id;

                    let keys = derive_key(
                        my_private_key,
                        responder_dh_public_key,
                        initiator_key_salt,
                        responder_key_salt,
                    ).map_err(|_| HandshakeError::InvalidKey)?;

                    sender_key = keys.0;
                    receiver_key = keys.1;
                } else {
                    sender_id = responder_id;
                    receiver_id = initiator_id;

                    let keys = derive_key(
                        my_private_key,
                        initiator_dh_public_key,
                        responder_key_salt,
                        initiator_key_salt,
                    ).map_err(|_| HandshakeError::InvalidKey)?;

                    sender_key = keys.0;
                    receiver_key = keys.1;
                }

                Ok(NewChannelInfo {
                    sender_id,
                    receiver_id,
                    sender_key,
                    receiver_key,
                    remote_public_key,
                })
            }
            _ => Err(HandshakeError::Incomplete)
        }
    }
}

impl HandshakeSessionMap {
    /// Creates an empty `HandshakeSessionMap`.
    pub fn new() -> HandshakeSessionMap {
        HandshakeSessionMap {
            in_flight_ids: HashSet::new(),
            last_hash_map: HashMap::new(),
        }
    }

    /// Insert a pair `(HashResult, HandshakeSession)` into the map.
    ///
    /// Returns `None` on success. On failure, means we have a in-flight
    /// session relevant to the "last hash" or "id", the input parameter
    /// will be returned and the map not changed.
    pub fn insert(&mut self, hash: HashResult, sess: HandshakeSession)
        -> Option<(HashResult, HandshakeSession)>
    {
        if !self.contains_id(&sess.id) && !self.contains_hash(&hash) {
            self.in_flight_ids.insert(sess.id.clone());
            self.last_hash_map.insert(hash, sess);

            None
        } else {
            Some((hash, sess))
        }
    }

    /// Check whether we have in-flight session relevant to given id.
    pub fn contains_id(&self, id: &HandshakeId) -> bool {
        self.in_flight_ids.contains(id)
    }

    /// Check whether we have in-flight session relevant to given hash.
    pub fn contains_hash(&self, h: &HashResult) -> bool {
        self.last_hash_map.contains_key(h)
    }

    /// Take a session with the **last message hash**, this will be used when we
    /// received a handshake relevant message, and need to advance its progress.
    ///
    /// # Panics
    ///
    /// Panics if the internal data inconsistent.
    pub fn take_by_hash(&mut self, h: &HashResult) -> Option<HandshakeSession> {
        self.last_hash_map.remove(h).and_then(|sess| {
            assert!(self.in_flight_ids.remove(&sess.id), "inconsistent status");
            Some(sess)
        })
    }

    /// Remove all in-flight sessions belong to given neighbor.
    pub fn remove_by_pk(&mut self, pk: &PublicKey) {
        self.remove_sessions(|sess| sess.remote_public_key() == pk);
    }

    /// Apply timer tick over all sessions, then remove timeout sessions.
    pub fn timer_tick(&mut self) -> usize {
        // Decrease timeout ticks for all sessions.
        for sess in self.last_hash_map.values_mut() {
            if sess.timeout_ticks > 0 {
                sess.timeout_ticks -= 1;
            }
        }
        self.remove_sessions(|sess| sess.timeout_ticks == 0)
    }

    pub fn len(&self) -> usize {
        self.last_hash_map.len()
    }

    fn remove_sessions<F>(&mut self, f: F) -> usize
        where
            F: Fn(&HandshakeSession) -> bool
    {
        let mut ids = Vec::new();
        self.last_hash_map.retain(|_, sess| {
            if f(sess) {
                ids.push(sess.id.clone());
                false
            } else {
                true
            }
        });
        for id in &ids {
            self.in_flight_ids.remove(id);
        }

        ids.len()
    }
}

// ==================== Helpers ====================

fn derive_key(
    my_private_key: DhPrivateKey,
    remote_dh_public_key: DhPublicKey,
    sent_salt: Salt,
    recv_salt: Salt,
) -> Result<(SealingKey, OpeningKey), HandshakeError> {
    let (send_key, recv_key) = my_private_key.derive_symmetric_key(
        remote_dh_public_key,
        sent_salt,
        recv_salt,
    ).map_err(|_| HandshakeError::InvalidKey)?;

    let sender_key = SealingKey::new(&CHACHA20_POLY1305, &send_key)
        .map_err(|_| HandshakeError::InvalidKey)?;

    let receiver_key = OpeningKey::new(&CHACHA20_POLY1305, &recv_key)
        .map_err(|_| HandshakeError::InvalidKey)?;

    Ok((sender_key, receiver_key))
}

fn derive_channel_id(
    initiator_rand_nonce: &RandValue,
    responder_rand_nonce: &RandValue,
    initiator_dh_public_key: &DhPublicKey,
    responder_dh_public_key: &DhPublicKey,
) -> (ChannelId, ChannelId) {
    let initiator_sender_id = {
        let mut data = Vec::new();
        data.extend_from_slice(b"Init");
        data.extend_from_slice(initiator_dh_public_key.as_ref());
        data.extend_from_slice(responder_dh_public_key.as_ref());
        data.extend_from_slice(initiator_rand_nonce.as_ref());
        data.extend_from_slice(responder_rand_nonce.as_ref());

        ChannelId::try_from(&sha_512_256(&data)[..CHANNEL_ID_LEN]).unwrap()
    };

    let responder_sender_id = {
        let mut data = Vec::new();
        data.extend_from_slice(b"Accp");
        data.extend_from_slice(responder_dh_public_key.as_ref());
        data.extend_from_slice(initiator_dh_public_key.as_ref());
        data.extend_from_slice(responder_rand_nonce.as_ref());
        data.extend_from_slice(initiator_rand_nonce.as_ref());

        ChannelId::try_from(&sha_512_256(&data)[..CHANNEL_ID_LEN]).unwrap()
    };

    (initiator_sender_id, responder_sender_id)
}

#[cfg(test)]
mod tests {
    use super::*;

    use crypto::rand_values::RAND_VALUE_LEN;
    use crypto::identity::PUBLIC_KEY_LEN;
    use crypto::hash::HASH_RESULT_LEN;
    use proto::channeler::InitChannel;

    const TIMEOUT_TICKS: usize = 10;

    #[test]
    fn session_map_insert_remove() {
        let mut sessions = HandshakeSessionMap::new();

        let init_channel = InitChannel {
            rand_nonce: RandValue::from(&[0x01; RAND_VALUE_LEN]),
            public_key: PublicKey::from(&[0x02; PUBLIC_KEY_LEN]),
        };

        let id = HandshakeId::new(HandshakeRole::Responder, init_channel.public_key.clone());

        let new_session = HandshakeSession {
            id: id.clone(),
            state: HandshakeState::RequestNonce,
            timeout_ticks: TIMEOUT_TICKS,
        };

        let last_hash = HashResult::from(&[0x03; HASH_RESULT_LEN]);

        assert!(sessions.insert(last_hash.clone(), new_session).is_none());

        assert!(sessions.contains_id(&id));
        assert!(sessions.contains_hash(&last_hash));

        assert!(sessions.take_by_hash(&last_hash).is_some());

        assert!(!sessions.contains_hash(&last_hash));
        assert!(!sessions.contains_id(&id));
    }

    #[test]
    fn session_map_timer_tick() {
        let mut sessions = HandshakeSessionMap::new();

        let init_channel1 = InitChannel {
            rand_nonce: RandValue::from(&[0x01; RAND_VALUE_LEN]),
            public_key: PublicKey::from(&[0x02; PUBLIC_KEY_LEN]),
        };
        let handshake_id1 = HandshakeId::new(
            HandshakeRole::Responder,
            init_channel1.public_key.clone(),
        );
        let new_session1 = HandshakeSession {
            id: handshake_id1.clone(),
            state: HandshakeState::InitChannel { init_channel: init_channel1 },
            timeout_ticks: TIMEOUT_TICKS,
        };
        let last_hash1 = HashResult::from(&[0x03; HASH_RESULT_LEN]);

        let init_channel2 = InitChannel {
            rand_nonce: RandValue::from(&[0x04; RAND_VALUE_LEN]),
            public_key: PublicKey::from(&[0x05; PUBLIC_KEY_LEN]),
        };
        let handshake_id2 = HandshakeId::new(
            HandshakeRole::Responder,
            init_channel2.public_key.clone(),
        );
        let new_session2 = HandshakeSession {
            id: handshake_id2.clone(),
            state: HandshakeState::InitChannel { init_channel: init_channel2 },
            timeout_ticks: TIMEOUT_TICKS + 1,
        };
        let last_hash2 = HashResult::from(&[0x06; HASH_RESULT_LEN]);

        assert!(sessions.insert(last_hash1.clone(), new_session1).is_none());
        assert!(sessions.insert(last_hash2.clone(), new_session2).is_none());

        assert!(sessions.contains_id(&handshake_id1));
        assert!(sessions.contains_hash(&last_hash1));
        assert!(sessions.contains_id(&handshake_id2));
        assert!(sessions.contains_hash(&last_hash2));

        // Remove nothing
        for _ in 0..TIMEOUT_TICKS - 1 {
            assert_eq!(sessions.timer_tick(), 0);
        }

        // Remove the session1
        assert_eq!(sessions.timer_tick(), 1);
        assert!(!sessions.contains_id(&handshake_id1));
        assert!(!sessions.contains_hash(&last_hash1));
        assert!(sessions.contains_id(&handshake_id2));
        assert!(sessions.contains_hash(&last_hash2));

        // Remove the session2
        assert_eq!(sessions.timer_tick(), 1);
        assert!(!sessions.contains_id(&handshake_id2));
        assert!(!sessions.contains_hash(&last_hash2));
    }

    #[test]
    fn session_map_insert_same_hash() {
        let mut sessions = HandshakeSessionMap::new();

        let init_channel1 = InitChannel {
            rand_nonce: RandValue::from(&[0x01; RAND_VALUE_LEN]),
            public_key: PublicKey::from(&[0x02; PUBLIC_KEY_LEN]),
        };
        let handshake_id1 = HandshakeId::new(
            HandshakeRole::Responder,
            init_channel1.public_key.clone(),
        );
        let new_session1 = HandshakeSession {
            id: handshake_id1.clone(),
            state: HandshakeState::InitChannel { init_channel: init_channel1 },
            timeout_ticks: TIMEOUT_TICKS,
        };
        let last_hash1 = HashResult::from(&[0x03; HASH_RESULT_LEN]);

        let init_channel2 = InitChannel {
            rand_nonce: RandValue::from(&[0x04; RAND_VALUE_LEN]),
            public_key: PublicKey::from(&[0x05; PUBLIC_KEY_LEN]),
        };
        let handshake_id2 = HandshakeId::new(
            HandshakeRole::Initiator,
            init_channel2.public_key.clone(),
        );
        let new_session2 = HandshakeSession {
            id: handshake_id2.clone(),
            state: HandshakeState::InitChannel { init_channel: init_channel2 },
            timeout_ticks: TIMEOUT_TICKS + 1,
        };

        assert!(sessions.insert(last_hash1.clone(), new_session1).is_none());
        assert!(sessions.insert(last_hash1.clone(), new_session2).is_some());
    }

    #[test]
    fn session_map_insert_same_id() {
        let mut sessions = HandshakeSessionMap::new();

        let init_channel1 = InitChannel {
            rand_nonce: RandValue::from(&[0x01; RAND_VALUE_LEN]),
            public_key: PublicKey::from(&[0x02; PUBLIC_KEY_LEN]),
        };
        let handshake_id = HandshakeId::new(
            HandshakeRole::Responder,
            init_channel1.public_key.clone(),
        );

        let new_session1 = HandshakeSession {
            id: handshake_id.clone(),
            state: HandshakeState::InitChannel { init_channel: init_channel1 },
            timeout_ticks: TIMEOUT_TICKS,
        };
        let last_hash1 = HashResult::from(&[0x03; HASH_RESULT_LEN]);

        let init_channel2 = InitChannel {
            rand_nonce: RandValue::from(&[0x04; RAND_VALUE_LEN]),
            public_key: PublicKey::from(&[0x05; PUBLIC_KEY_LEN]),
        };
        let new_session2 = HandshakeSession {
            id: handshake_id.clone(),
            state: HandshakeState::InitChannel { init_channel: init_channel2 },
            timeout_ticks: TIMEOUT_TICKS + 1,
        };

        assert!(sessions.insert(last_hash1, new_session1).is_none());

        let last_hash2 = HashResult::from(&[0x06; HASH_RESULT_LEN]);

        assert!(sessions.insert(last_hash2, new_session2).is_some());
    }

    #[test]
    fn session_map_remove_pk() {
        let mut sessions = HandshakeSessionMap::new();

        let public_key = PublicKey::from(&[0x00; PUBLIC_KEY_LEN]);

        let init_channel1 = InitChannel {
            rand_nonce: RandValue::from(&[0x01; RAND_VALUE_LEN]),
            public_key: public_key.clone(),
        };
        let new_session1 = HandshakeSession {
            id: HandshakeId::new(HandshakeRole::Responder, public_key.clone()),
            state: HandshakeState::InitChannel { init_channel: init_channel1 },
            timeout_ticks: TIMEOUT_TICKS,
        };
        let last_hash1 = HashResult::from(&[0x02; HASH_RESULT_LEN]);

        let init_channel2 = InitChannel {
            rand_nonce: RandValue::from(&[0x03; RAND_VALUE_LEN]),
            public_key: public_key.clone(),
        };
        let new_session2 = HandshakeSession {
            id: HandshakeId::new(HandshakeRole::Initiator, public_key.clone()),
            state: HandshakeState::InitChannel { init_channel: init_channel2 },
            timeout_ticks: TIMEOUT_TICKS + 1,
        };
        let last_hash2 = HashResult::from(&[0x04; HASH_RESULT_LEN]);

        assert!(sessions.insert(last_hash1, new_session1).is_none());
        assert!(sessions.insert(last_hash2, new_session2).is_none());

        assert_eq!(sessions.len(), 2);

        sessions.remove_by_pk(&public_key);

        assert_eq!(sessions.len(), 0);
    }
}
