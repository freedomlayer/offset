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

use super::state::HandshakeState;
use super::error::HandshakeError;

#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub enum HandshakeRole {
    Initiator,
    Responder,
}

#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub struct HandshakeId(HandshakeRole, PublicKey);

pub struct HandshakeSession {
    id: HandshakeId,
    pub state: HandshakeState,
    session_timeout: usize,
}

pub struct HandshakeSessionMap {
    in_flight_ids: HashSet<HandshakeId>,
    last_hash_map: HashMap<HashResult, HandshakeSession>,
}

pub struct NewChannelInfo {
    pub sender_id: ChannelId,
    pub sender_key: SealingKey,
    pub receiver_id: ChannelId,
    pub receiver_key: OpeningKey,
    pub remote_public_key: PublicKey,
}

impl HandshakeId {
    pub fn new(role: HandshakeRole, remote_pk: PublicKey) -> HandshakeId {
        HandshakeId(role, remote_pk)
    }

    #[inline]
    fn is_initiator(&self) -> bool {
        match self.0 {
            HandshakeRole::Initiator => true,
            HandshakeRole::Responder => false,
        }
    }

    #[inline]
    fn remote_public_key(&self) -> &PublicKey {
        &self.1
    }
}

impl HandshakeSession {
    pub fn new(id: HandshakeId, state: HandshakeState, timeout_ticks: usize)
        -> HandshakeSession
    {
        HandshakeSession { id, state, session_timeout: timeout_ticks }
    }

    #[inline]
    pub fn is_initiator(&self) -> bool {
        self.id.is_initiator()
    }

    #[inline]
    pub fn remote_public_key(&self) -> &PublicKey {
        self.id.remote_public_key()
    }

    pub fn state(&self) -> &HandshakeState {
        &self.state
    }

    pub fn state_mut(&mut self) -> &mut HandshakeState {
        &mut self.state
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
                recv_rand_nonce,
                sent_rand_nonce,
                recv_key_salt,
                sent_key_salt,
                remote_dh_public_key,
                my_private_key,
            } => {
                let my_dh_public_key = my_private_key.compute_public_key()
                    .map_err(HandshakeError::Crypto)?;

                let (sender_id, receiver_id) = derive_channel_id(
                    &sent_rand_nonce,
                    &recv_rand_nonce,
                    &my_dh_public_key,
                    &remote_dh_public_key
                );

                let (sender_key, receiver_key) = derive_key(
                    my_private_key,
                    remote_dh_public_key,
                    sent_key_salt,
                    recv_key_salt,
                ).map_err(|_| HandshakeError::InvalidKey)?;

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
            if sess.session_timeout > 0 {
                sess.session_timeout -= 1;
            }
        }
        self.remove_sessions(|sess| sess.session_timeout == 0)
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
    sent_rand_nonce: &RandValue,
    recv_rand_nonce: &RandValue,
    sent_dh_public_key: &DhPublicKey,
    recv_dh_public_key: &DhPublicKey,
) -> (ChannelId, ChannelId) {
    let sender_id = {
        let mut data = Vec::new();
        // data.extend_from_slice(b"Init");
        data.extend_from_slice(sent_dh_public_key.as_ref());
        data.extend_from_slice(recv_dh_public_key.as_ref());
        data.extend_from_slice(sent_rand_nonce.as_ref());
        data.extend_from_slice(recv_rand_nonce.as_ref());

        ChannelId::try_from(&sha_512_256(&data)[..CHANNEL_ID_LEN]).unwrap()
    };

    let receiver_id = {
        let mut data = Vec::new();
        // data.extend_from_slice(b"Accp");
        data.extend_from_slice(recv_dh_public_key.as_ref());
        data.extend_from_slice(sent_dh_public_key.as_ref());
        data.extend_from_slice(recv_rand_nonce.as_ref());
        data.extend_from_slice(sent_rand_nonce.as_ref());

        ChannelId::try_from(&sha_512_256(&data)[..CHANNEL_ID_LEN]).unwrap()
    };

    (sender_id, receiver_id)
}

#[cfg(test)]
mod tests {
    use super::*;

    use crypto::rand_values::RAND_VALUE_LEN;
    use crypto::identity::PUBLIC_KEY_LEN;
    use crypto::hash::HASH_RESULT_LEN;

    const TIMEOUT_TICKS: usize = 10;

    #[test]
    fn session_map_insert_remove() {
        let mut sessions = HandshakeSessionMap::new();

        let remote_public_key = PublicKey::from(&[0x00; PUBLIC_KEY_LEN]);
        let id = HandshakeId::new(HandshakeRole::Responder, remote_public_key);

        let new_session = HandshakeSession {
            id: id.clone(),
            state: HandshakeState::InitiatorRequestNonce,
            session_timeout: TIMEOUT_TICKS,
        };

        let last_hash = HashResult::from(&[0x01; HASH_RESULT_LEN]);

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

        let remote_public_key1 = PublicKey::from(&[0x00; PUBLIC_KEY_LEN]);
        let handshake_id1 = HandshakeId::new(HandshakeRole::Responder, remote_public_key1);
        let new_session1 = HandshakeSession {
            id: handshake_id1.clone(),
            state: HandshakeState::InitiatorRequestNonce,
            session_timeout: TIMEOUT_TICKS,
        };
        let last_hash1 = HashResult::from(&[0x01; HASH_RESULT_LEN]);

        let remote_public_key2 = PublicKey::from(&[0x02; PUBLIC_KEY_LEN]);
        let handshake_id2 = HandshakeId::new(HandshakeRole::Responder, remote_public_key2);
        let new_session2 = HandshakeSession {
            id: handshake_id2.clone(),
            state: HandshakeState::InitiatorRequestNonce,
            session_timeout: TIMEOUT_TICKS + 1,
        };
        let last_hash2 = HashResult::from(&[0x03; HASH_RESULT_LEN]);

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

        let remote_public_key1 = PublicKey::from(&[0x00; PUBLIC_KEY_LEN]);
        let handshake_id1 = HandshakeId::new(HandshakeRole::Responder, remote_public_key1);
        let new_session1 = HandshakeSession {
            id: handshake_id1.clone(),
            state: HandshakeState::InitiatorRequestNonce,
            session_timeout: TIMEOUT_TICKS,
        };

        let last_hash1 = HashResult::from(&[0x01; HASH_RESULT_LEN]);

        let remote_public_key2 = PublicKey::from(&[0x02; PUBLIC_KEY_LEN]);
        let handshake_id2 = HandshakeId::new(HandshakeRole::Responder, remote_public_key2);
        let new_session2 = HandshakeSession {
            id: handshake_id2.clone(),
            state: HandshakeState::InitiatorRequestNonce,
            session_timeout: TIMEOUT_TICKS + 1,
        };

        assert!(sessions.insert(last_hash1.clone(), new_session1).is_none());
        assert!(sessions.insert(last_hash1.clone(), new_session2).is_some());
    }

    #[test]
    fn session_map_insert_same_id() {
        let mut sessions = HandshakeSessionMap::new();

        let remote_public_key = PublicKey::from(&[0x00; PUBLIC_KEY_LEN]);
        let handshake_id1 = HandshakeId::new(HandshakeRole::Responder, remote_public_key.clone());
        let new_session1 = HandshakeSession {
            id: handshake_id1.clone(),
            state: HandshakeState::InitiatorRequestNonce,
            session_timeout: TIMEOUT_TICKS,
        };
        let last_hash1 = HashResult::from(&[0x01; HASH_RESULT_LEN]);

        let handshake_id2 = HandshakeId::new(HandshakeRole::Responder, remote_public_key);
        let new_session2 = HandshakeSession {
            id: handshake_id2.clone(),
            state: HandshakeState::InitiatorRequestNonce,
            session_timeout: TIMEOUT_TICKS + 1,
        };
        let last_hash2 = HashResult::from(&[0x03; HASH_RESULT_LEN]);

        assert!(sessions.insert(last_hash1, new_session1).is_none());

        let last_hash2 = HashResult::from(&[0x06; HASH_RESULT_LEN]);

        assert!(sessions.insert(last_hash2, new_session2).is_some());
    }

    #[test]
    fn session_map_remove_pk() {
        let mut sessions = HandshakeSessionMap::new();

        let remote_public_key = PublicKey::from(&[0x00; PUBLIC_KEY_LEN]);
        let handshake_id1 = HandshakeId::new(HandshakeRole::Responder, remote_public_key.clone());
        let new_session1 = HandshakeSession {
            id: handshake_id1.clone(),
            state: HandshakeState::InitiatorRequestNonce,
            session_timeout: TIMEOUT_TICKS,
        };
        let last_hash1 = HashResult::from(&[0x01; HASH_RESULT_LEN]);

        let handshake_id2 = HandshakeId::new(HandshakeRole::Initiator, remote_public_key.clone());
        let new_session2 = HandshakeSession {
            id: handshake_id2.clone(),
            state: HandshakeState::InitiatorRequestNonce,
            session_timeout: TIMEOUT_TICKS + 1,
        };
        let last_hash2 = HashResult::from(&[0x03; HASH_RESULT_LEN]);

        assert!(sessions.insert(last_hash1, new_session1).is_none());
        assert!(sessions.insert(last_hash2, new_session2).is_none());

        assert_eq!(sessions.len(), 2);

        sessions.remove_by_pk(&remote_public_key);

        assert_eq!(sessions.len(), 0);
    }
}
