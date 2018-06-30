use std::convert::TryFrom;

use ring::aead::{CHACHA20_POLY1305, SealingKey, OpeningKey};

use crypto::identity::PublicKey;
use crypto::rand_values::RandValue;
use crypto::hash::{sha_512_256, HashResult};
use crypto::dh::{DhPublicKey, DhPrivateKey, Salt};
use channeler::config::{HANDSHAKE_SESSION_TIMEOUT, REQUEST_NONCE_TIMEOUT};
use proto::channeler::{ChannelId, CHANNEL_ID_LEN};

use super::ChannelMetadata;
use super::state::HandshakeState;
use super::error::Error;

mod session_table;

pub use self::session_table::SessionTable;

#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub enum HandshakeRole {
    Initiator,
    Responder,
}

#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub struct SessionId(HandshakeRole, PublicKey);

pub struct RequestNonceSession {
    remote_public_key: PublicKey,
    pub timeout: usize,
}

pub struct HandshakeSession<S = HandshakeState> {
    session_id: SessionId,
    state: S,
    last_hash: HashResult,
    session_timeout: usize,
}

impl SessionId {
    pub fn new_initiator(remote_public_key: PublicKey) -> SessionId {
        SessionId(HandshakeRole::Initiator, remote_public_key)
    }

    pub fn new_responder(remote_public_key: PublicKey) -> SessionId {
        SessionId(HandshakeRole::Responder, remote_public_key)
    }

    #[inline]
    pub fn is_initiator(&self) -> bool {
        match self.0 {
            HandshakeRole::Initiator => true,
            HandshakeRole::Responder => false,
        }
    }

    #[inline]
    pub fn is_responder(&self) -> bool {
        match self.0 {
            HandshakeRole::Initiator => false,
            HandshakeRole::Responder => true,
        }
    }
}

impl RequestNonceSession {
    pub fn new(remote_public_key: PublicKey) -> RequestNonceSession {
        RequestNonceSession {
            remote_public_key,
            timeout: REQUEST_NONCE_TIMEOUT,
        }
    }

    #[inline]
    pub fn remote_public_key(&self) -> &PublicKey {
        &self.remote_public_key
    }

    pub fn finish(self) -> PublicKey {
        self.remote_public_key
    }
}

impl<S> HandshakeSession<S> {
    pub fn new(sid: SessionId, state: S, hash: HashResult) -> HandshakeSession<S> {
        HandshakeSession {
            session_id: sid,
            state,
            last_hash: hash,
            session_timeout: HANDSHAKE_SESSION_TIMEOUT,
        }
    }

    #[inline]
    pub fn session_id(&self) -> &SessionId {
        &self.session_id
    }

    #[inline]
    pub fn remote_public_key(&self) -> &PublicKey {
        &self.session_id.1
    }

    #[inline]
    pub fn is_initiator(&self) -> bool {
        self.session_id().is_initiator()
    }
}

impl HandshakeSession {
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

    pub fn initiator_finish(self, recv_key_salt: Salt, remote_dh_public_key: DhPublicKey) -> Result<ChannelMetadata, Error> {
        match self.state {
            HandshakeState::AfterInitiatorExchangeActive {
                recv_rand_nonce,
                sent_rand_nonce,
                sent_key_salt,
                local_dh_private_key
            } => {
                finish(
                    sent_rand_nonce,
                    recv_rand_nonce,
                    sent_key_salt,
                    recv_key_salt,
                    remote_dh_public_key,
                    local_dh_private_key,
                    self.session_id.1,
                )
            }

            _ => panic!("call initiator_finish when the state not match!")
        }
    }

    pub fn responder_finish(self) -> Result<ChannelMetadata, Error> {
        match self.state {
            HandshakeState::AfterResponderExchangePassive {
                sent_rand_nonce,
                recv_rand_nonce,
                sent_key_salt,
                recv_key_salt,
                remote_dh_public_key,
                local_dh_private_key,
            } => {
                finish(
                    sent_rand_nonce,
                    recv_rand_nonce,
                    sent_key_salt,
                    recv_key_salt,
                    remote_dh_public_key,
                    local_dh_private_key,
                    self.session_id.1,
                )
            }

            _ => panic!("call responder_finish when the state not match, this is a bug!")
        }
    }
}

// ===== utils =====

fn finish(
    sent_rand_nonce: RandValue,
    recv_rand_nonce: RandValue,
    sent_key_salt: Salt,
    recv_key_salt: Salt,
    remote_dh_public_key: DhPublicKey,
    my_private_key: DhPrivateKey,
    remote_public_key: PublicKey,
) -> Result<ChannelMetadata, Error> {
    let (tx_cid, rx_cid) = derive_channel_id(
        &sent_rand_nonce,
        &recv_rand_nonce,
        &my_private_key.compute_public_key().unwrap(),
        &remote_dh_public_key,
    );
    let (tx_key, rx_key) = derive_key(
        my_private_key,
        remote_dh_public_key,
        sent_key_salt,
        recv_key_salt
    )?;

    Ok(ChannelMetadata {
        remote_public_key,
        tx_cid, tx_key,
        rx_cid, rx_key,
    })
}

fn derive_channel_id(
    sent_rand_nonce: &RandValue,
    recv_rand_nonce: &RandValue,
    sent_dh_public_key: &DhPublicKey,
    recv_dh_public_key: &DhPublicKey,
) -> (ChannelId, ChannelId) {
    let tx_cid = {
        let mut data = Vec::new();
        // data.extend_from_slice(b"Init");
        data.extend_from_slice(sent_dh_public_key.as_ref());
        data.extend_from_slice(recv_dh_public_key.as_ref());
        data.extend_from_slice(sent_rand_nonce.as_ref());
        data.extend_from_slice(recv_rand_nonce.as_ref());

        ChannelId::try_from(&sha_512_256(&data)[..CHANNEL_ID_LEN]).unwrap()
    };

    let rx_cid = {
        let mut data = Vec::new();
        // data.extend_from_slice(b"Accp");
        data.extend_from_slice(recv_dh_public_key.as_ref());
        data.extend_from_slice(sent_dh_public_key.as_ref());
        data.extend_from_slice(recv_rand_nonce.as_ref());
        data.extend_from_slice(sent_rand_nonce.as_ref());

        ChannelId::try_from(&sha_512_256(&data)[..CHANNEL_ID_LEN]).unwrap()
    };

    (tx_cid, rx_cid)
}

fn derive_key(
    local_dh_private_key: DhPrivateKey,
    remote_dh_public_key: DhPublicKey,
    sent_key_salt: Salt,
    recv_key_salt: Salt,
) -> Result<(SealingKey, OpeningKey), Error> {
    let (tx_sym_key, rx_sym_key) = local_dh_private_key
        .derive_symmetric_key(remote_dh_public_key, sent_key_salt, recv_key_salt)
        .map_err(|_| Error::DeriveSymmetricKeyFailed)?;

    let tx_key = SealingKey::new(&CHACHA20_POLY1305, &tx_sym_key)
        .map_err(|_| Error::ConvertToSealingKeyFailed)?;

    let rx_key = OpeningKey::new(&CHACHA20_POLY1305, &rx_sym_key)
        .map_err(|_| Error::ConvertToOpeningKeyFailed)?;

    Ok((tx_key, rx_key))
}
