use std::collections::{HashMap, HashSet};
use std::convert::TryFrom;

use ring::aead::{CHACHA20_POLY1305, OpeningKey, SealingKey};

use crypto::dh::DhPrivateKey;
use crypto::hash::{HashResult, sha_512_256};
use crypto::identity::PublicKey;
use proto::channeler_udp::{ExchangeActive, ExchangePassive, InitChannel};

use super::{ChannelId, HandshakeManagerError, NewChannelInfo, CHANNEL_ID_LEN};

#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub enum HandshakeRole {
    Initiator,
    Responder,
}

#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub struct HandshakeId {
    role:              HandshakeRole,
    remote_public_key: PublicKey,
}

impl HandshakeId {
    /// Creates a `HandshakeSessionId` with `PublicKey` and `HandshakeRole`
    pub fn new(role: HandshakeRole, remote_public_key: PublicKey) -> HandshakeId {
        HandshakeId {
            role,
            remote_public_key,
        }
    }

    #[inline]
    fn remote_public_key(&self) -> &PublicKey {
        &self.remote_public_key
    }

    #[inline]
    fn is_initiator(&self) -> bool {
        match self.role {
            HandshakeRole::Initiator => true,
            HandshakeRole::Responder => false,
        }
    }
}

pub struct Handshake {
    pub id:            HandshakeId,
    pub timeout_ticks: usize,

    pub init_channel:     Option<InitChannel>,
    pub my_private_key:   Option<DhPrivateKey>,
    pub exchange_active:  Option<ExchangeActive>,
    pub exchange_passive: Option<ExchangePassive>,
}

impl Handshake {
    /// Returns a reference to the remote public key
    pub fn remote_public_key(&self) -> &PublicKey {
        self.id.remote_public_key()
    }

    /// Returns [`true`] if this session is initiated by local
    pub fn is_initiator(&self) -> bool {
        self.id.is_initiator()
    }

    /// Finish the 4-way handshake and derive the new channel information
    ///
    /// - For the **initiator**, this method **SHOULD** be called after the
    /// `HandshakeManager` finish validating the `ChannelReady` message.
    ///
    /// - For the **responder**, this method **SHOULD** be called after
    /// the `ExchangeActive` arrival, and it passed the validation.
    pub fn finish(self) -> Result<NewChannelInfo, HandshakeManagerError> {
        let init_channel = self.init_channel
            .ok_or(HandshakeManagerError::InvalidHandshake)?;
        let my_private_key = self.my_private_key
            .ok_or(HandshakeManagerError::InvalidHandshake)?;
        let exchange_active = self.exchange_active
            .ok_or(HandshakeManagerError::InvalidHandshake)?;
        let exchange_passive = self.exchange_passive
            .ok_or(HandshakeManagerError::InvalidHandshake)?;

        // `ChannelId` to identify the channel from initiator to responder
        //
        // - The channel !!initiator!! keeps this id in its sending end
        // - The channel !!responder!! keeps this id in its receiving end
        let initiator_sender_id = {
            let mut data = Vec::new();
            data.extend_from_slice(b"Init");
            data.extend_from_slice(exchange_active.dh_public_key.as_ref());
            data.extend_from_slice(exchange_passive.dh_public_key.as_ref());
            data.extend_from_slice(init_channel.rand_nonce.as_ref());
            data.extend_from_slice(exchange_passive.rand_nonce.as_ref());

            ChannelId::try_from(&sha_512_256(&data).as_ref()[..CHANNEL_ID_LEN])
                .map_err(|_| HandshakeManagerError::Other)?
        };

        // `ChannelId` to identify the channel from responder to initiator
        //
        // - The channel !!responder!! keeps this id in its sending end
        // - The channel !!initiator!! keeps this id in its receiving end
        let responder_sender_id = {
            let mut data = Vec::new();
            data.extend_from_slice(b"Accp");
            data.extend_from_slice(exchange_passive.dh_public_key.as_ref());
            data.extend_from_slice(exchange_active.dh_public_key.as_ref());
            data.extend_from_slice(exchange_passive.rand_nonce.as_ref());
            data.extend_from_slice(init_channel.rand_nonce.as_ref());

            ChannelId::try_from(&sha_512_256(&data).as_ref()[..CHANNEL_ID_LEN])
                .map_err(|_| HandshakeManagerError::Other)?
        };

        // The symmetric key for !!initiator!! to send message to !!responder!!
        let initiator_sending_key = my_private_key
            .derive_symmetric_key(&exchange_passive.dh_public_key, &exchange_active.key_salt);

        // The symmetric key for !!responder!! to send message to !!initiator!!
        let responder_sending_key = my_private_key
            .derive_symmetric_key(&exchange_active.dh_public_key, &exchange_passive.key_salt);

        if self.id.is_initiator() {
            let sending_key = SealingKey::new(&CHACHA20_POLY1305, initiator_sending_key.as_bytes())
                .map_err(|_| HandshakeManagerError::Other)?;
            let receiving_key =
                OpeningKey::new(&CHACHA20_POLY1305, responder_sending_key.as_bytes())
                    .map_err(|_| HandshakeManagerError::Other)?;

            Ok(NewChannelInfo {
                receiver_id:  responder_sender_id,
                sender_id:    initiator_sender_id,
                sender_key:   sending_key,
                receiver_key: receiving_key,
            })
        } else {
            let sending_key = SealingKey::new(&CHACHA20_POLY1305, responder_sending_key.as_bytes())
                .map_err(|_| HandshakeManagerError::Other)?;
            let receiving_key =
                OpeningKey::new(&CHACHA20_POLY1305, initiator_sending_key.as_bytes())
                    .map_err(|_| HandshakeManagerError::Other)?;

            Ok(NewChannelInfo {
                receiver_id:  initiator_sender_id,
                sender_id:    responder_sender_id,
                sender_key:   sending_key,
                receiver_key: receiving_key,
            })
        }
    }
}

pub struct HandshakeTable {
    session_id_set:  HashSet<HandshakeId>,
    hash_result_map: HashMap<HashResult, Handshake>,
}

impl HandshakeTable {
    pub fn new() -> HandshakeTable {
        HandshakeTable {
            session_id_set:  HashSet::new(),
            hash_result_map: HashMap::new(),
        }
    }

    pub fn insert(&mut self, hash: HashResult, session: Handshake) {
        assert!(self.session_id_set.insert(session.id.clone()));
        assert!(self.hash_result_map.insert(hash, session).is_none());
    }

    pub fn contains(&self, hash: &HashResult) -> bool {
        self.hash_result_map.contains_key(hash)
    }

    pub fn contains_id(&self, id: &HandshakeId) -> bool {
        self.session_id_set.contains(id)
    }

    pub fn remove(&mut self, hash: &HashResult) -> Option<Handshake> {
        self.hash_result_map.remove(hash).and_then(|session| {
            assert!(self.session_id_set.remove(&session.id));
            Some(session)
        })
    }

    pub fn tick(&mut self) -> usize {
        let before = self.session_id_set.len();

        for session in self.hash_result_map.values_mut() {
            if session.timeout_ticks <= 1 {
                session.timeout_ticks = 0;
                assert!(self.session_id_set.remove(&session.id));
            } else {
                session.timeout_ticks -= 1;
            }
        }

        self.hash_result_map
            .retain(|_, session| session.timeout_ticks >= 1);

        assert_eq!(self.session_id_set.len(), self.hash_result_map.len());

        before - self.session_id_set.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crypto::hash::sha_512_256;
    use crypto::identity::{PublicKey, PUBLIC_KEY_LEN};

    const TICKS: usize = 100;

    fn gen_handshake_session(seed: &[u8]) -> (HashResult, Handshake) {
        let hash = sha_512_256(seed);
        let remote_public_key = PublicKey::from_bytes(&[seed[0]; PUBLIC_KEY_LEN]).unwrap();
        let id = HandshakeId::new(HandshakeRole::Initiator, remote_public_key);

        let new_session = Handshake {
            id,
            timeout_ticks: TICKS,
            init_channel: None,
            my_private_key: None,
            exchange_active: None,
            exchange_passive: None,
        };

        (hash, new_session)
    }

    #[test]
    fn test_insert() {
        let mut handshake_session_table = HandshakeTable::new();

        let (hash, new_session) = gen_handshake_session(&[0, 1, 2][..]);

        let new_session_id = new_session.id.clone();

        handshake_session_table.insert(hash.clone(), new_session);

        assert!(handshake_session_table.contains(&hash));
        assert!(handshake_session_table.contains_id(&new_session_id));

        assert!(handshake_session_table.remove(&hash).is_some());

        assert!(!handshake_session_table.contains(&hash));
        assert!(!handshake_session_table.contains_id(&new_session_id));
    }

    #[test]
    #[should_panic]
    fn test_insert_with_same_key() {
        let mut handshake_session_table = HandshakeTable::new();

        let (hash, new_session1) = gen_handshake_session(&[0, 1, 2][..]);
        let (_hash, new_session2) = gen_handshake_session(&[1, 2, 3][..]);

        handshake_session_table.insert(hash.clone(), new_session1);
        handshake_session_table.insert(hash.clone(), new_session2);
    }

    #[test]
    fn test_tick() {
        let mut handshake_session_table = HandshakeTable::new();

        let (hash1, new_session1) = gen_handshake_session(&[0, 1, 2][..]);
        let (hash2, mut new_session2) = gen_handshake_session(&[1, 2, 3][..]);

        new_session2.timeout_ticks += 1;

        handshake_session_table.insert(hash1.clone(), new_session1);
        handshake_session_table.insert(hash2.clone(), new_session2);

        for _ in 0..TICKS {
            handshake_session_table.tick();
        }

        assert!(!handshake_session_table.contains(&hash1));
        assert!(handshake_session_table.contains(&hash2));
        assert_eq!(handshake_session_table.tick(), 1);
    }
}
