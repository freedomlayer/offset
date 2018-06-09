use std::convert::TryFrom;

use ring::aead::{CHACHA20_POLY1305, SealingKey, OpeningKey};

use crypto::identity::PublicKey;
use crypto::rand_values::RandValue;
use crypto::hash::{sha_512_256, HashResult};
use crypto::dh::{DhPublicKey, DhPrivateKey, Salt};
use channeler::config::HANDSHAKE_SESSION_TIMEOUT;
use proto::channeler::{ChannelId, CHANNEL_ID_LEN};

pub(super) mod session_table;

use super::HandshakeResult;
use super::state::HandshakeState;
use super::error::HandshakeError;

#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub enum HandshakeRole {
    Initiator,
    Responder,
}

#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub struct SessionId(HandshakeRole, PublicKey);

pub struct HandshakeSession<S = HandshakeState> {
    id: SessionId,
    state: S,
    last_hash: HashResult,
    timeout_counter: usize,
}

impl SessionId {
    /// Creates a new `HandshakeId` with "initiator" role.
    pub fn new_initiator(remote_public_key: PublicKey) -> SessionId {
        SessionId(HandshakeRole::Initiator, remote_public_key)
    }

    /// Creates a new `HandshakeId` with "responder" role.
    pub fn new_responder(remote_public_key: PublicKey) -> SessionId {
        SessionId(HandshakeRole::Responder, remote_public_key)
    }

    pub fn is_initiator(&self) -> bool {
        match self.0 {
            HandshakeRole::Initiator => true,
            HandshakeRole::Responder => false,
        }
    }

    pub fn is_responder(&self) -> bool {
        match self.0 {
            HandshakeRole::Initiator => false,
            HandshakeRole::Responder => true,
        }
    }
}

impl<S> HandshakeSession<S> {
    /// Creates a new handshake session.
    pub fn new(id: SessionId, state: S, last_hash: HashResult) -> HandshakeSession<S> {
        HandshakeSession { id, state, last_hash, timeout_counter: HANDSHAKE_SESSION_TIMEOUT }
    }

    /// Returns a reference to the session id.
    #[inline]
    pub fn session_id(&self) -> &SessionId {
        &self.id
    }

    /// Returns a reference to the remote's public key.
    #[inline]
    pub fn remote_public_key(&self) -> &PublicKey {
        &self.id.1
    }

    #[inline]
    pub fn is_initiator(&self) -> bool {
        self.session_id().is_initiator()
    }
}

impl HandshakeSession {
    /// Returns a reference to current handshake state.
    #[inline]
    pub fn state(&self) -> &HandshakeState {
        &self.state
    }

    #[inline]
    pub fn is_after_initiator_exchange_active(&self) -> bool {
        self.state().is_after_initiator_exchange_active()
    }

    #[inline]
    pub fn is_after_responder_exchange_passive(&self) -> bool {
        self.state().is_after_responder_exchange_passive()
    }

    pub fn initiator_finish(self, recv_key_salt: Salt, peer_dh_public_key: DhPublicKey)
        -> Result<HandshakeResult, HandshakeError>
    {
        match self.state {
            HandshakeState::AfterInitiatorExchangeActive {
                recv_rand_nonce,
                sent_rand_nonce,
                sent_key_salt,
                my_private_key
            } => {
                finish(
                    sent_rand_nonce,
                    recv_rand_nonce,
                    sent_key_salt,
                    recv_key_salt,
                    peer_dh_public_key,
                    my_private_key,
                    self.id.1
                )
            },

            _ => panic!("call initiator_finish when the state not match, this is a bug!")
        }
    }

    pub fn responder_finish(self) -> Result<HandshakeResult, HandshakeError> {
        match self.state {
            HandshakeState::AfterResponderExchangePassive {
                sent_rand_nonce,
                recv_rand_nonce,
                sent_key_salt,
                recv_key_salt,
                remote_dh_public_key,
                my_private_key,
            } => {
                finish(
                    sent_rand_nonce,
                    recv_rand_nonce,
                    sent_key_salt,
                    recv_key_salt,
                    remote_dh_public_key,
                    my_private_key,
                    self.id.1
                )
            }

            _ => panic!("call responder_finish when the state not match, this is a bug!")
        }
    }
}

// ===== utils =====

fn finish(sent_rand_nonce: RandValue,
          recv_rand_nonce: RandValue,
          sent_key_salt: Salt,
          recv_key_salt: Salt,
          remote_dh_public_key: DhPublicKey,
          my_private_key: DhPrivateKey,
          remote_public_key: PublicKey,
) -> Result<HandshakeResult, HandshakeError> {
    let (sending_channel_id, receiving_channel_id) =
        derive_channel_id(&sent_rand_nonce, &recv_rand_nonce, &my_private_key.compute_public_key().unwrap(), &remote_dh_public_key);
    let (sending_channel_key, receiving_channel_key) =
        derive_key(my_private_key, remote_dh_public_key, sent_key_salt, recv_key_salt)?;

    Ok(HandshakeResult {
        remote_public_key,
        channel_tx_id: sending_channel_id,
        channel_tx_key: sending_channel_key,
        channel_rx_id: receiving_channel_id,
        channel_rx_key: receiving_channel_key
    })
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

fn derive_key(
    my_private_key: DhPrivateKey,
    peer_dh_public_key: DhPublicKey,
    sent_key_salt: Salt,
    recv_key_salt: Salt,
) -> Result<(SealingKey, OpeningKey), HandshakeError> {
    let (sending_channel_key, receiving_channel_key) = my_private_key
        .derive_symmetric_key(peer_dh_public_key, sent_key_salt, recv_key_salt)
        .map_err(|_| HandshakeError::InvalidKey)?;

    let sender_key = SealingKey::new(&CHACHA20_POLY1305, &sending_channel_key)
        .map_err(|_| HandshakeError::InvalidKey)?;

    let receiver_key = OpeningKey::new(&CHACHA20_POLY1305, &receiving_channel_key)
        .map_err(|_| HandshakeError::InvalidKey)?;

    Ok((sender_key, receiver_key))
}
