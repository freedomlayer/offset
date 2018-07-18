use std::{cell::RefCell, collections::HashMap, rc::Rc};

use ring::rand::SecureRandom;

use crypto::{
    dh::{DhPrivateKey, DhPublicKey, Salt},
    hash::{sha_512_256, HashResult},
    identity::{verify_signature, PublicKey, Signature, SIGNATURE_LEN},
    rand_values::RandValue,
};
use proto::channeler::*;

use super::{
    super::{
        config::{HANDSHAKE_SESSION_TIMEOUT, REQUEST_NONCE_TIMEOUT},
        types::NeighborTable,
    },
    utils::finish_handshake,
    ChannelMetadata,
    HandshakeError,
};

pub struct HandshakeClient<SR> {
    local_public_key: PublicKey,
    neighbors:        Rc<RefCell<NeighborTable>>,
    secure_rng:       Rc<SR>,

    request_nonce_sessions: HashMap<RandValue, RequestNonceSession>,

    public_key_to_hash_result: HashMap<PublicKey, HashResult>,
    handshake_client_sessions: HashMap<HashResult, HandshakeClientSession>,
}

struct RequestNonceSession {
    remote_public_key: PublicKey,
    timeout:           usize,
}

impl RequestNonceSession {
    fn new(remote_public_key: PublicKey) -> RequestNonceSession {
        RequestNonceSession {
            remote_public_key,
            timeout: REQUEST_NONCE_TIMEOUT,
        }
    }

    fn remote_public_key(&self) -> &PublicKey {
        &self.remote_public_key
    }

    fn finish(self) -> PublicKey {
        self.remote_public_key
    }
}

struct HandshakeClientSession {
    remote_public_key: PublicKey,

    recv_rand_nonce:      RandValue,
    sent_rand_nonce:      RandValue,
    sent_key_salt:        Salt,
    local_dh_private_key: DhPrivateKey,

    timeout: usize,
}

impl HandshakeClientSession {
    #[inline]
    fn remote_public_key(&self) -> &PublicKey {
        &self.remote_public_key
    }

    fn finish(
        self,
        recv_key_salt: Salt,
        remote_dh_public_key: DhPublicKey,
    ) -> Result<ChannelMetadata, HandshakeError> {
        finish_handshake(
            self.remote_public_key,
            self.sent_rand_nonce,
            self.recv_rand_nonce,
            self.sent_key_salt,
            recv_key_salt,
            remote_dh_public_key,
            self.local_dh_private_key
        )
    }
}

impl<SR: SecureRandom> HandshakeClient<SR> {
    pub fn new(
        local_public_key: PublicKey,
        neighbors: Rc<RefCell<NeighborTable>>,
        rng: Rc<SR>,
    ) -> HandshakeClient<SR> {
        HandshakeClient {
            local_public_key,
            neighbors,
            secure_rng: rng,

            request_nonce_sessions: HashMap::new(),
            public_key_to_hash_result: HashMap::new(),
            handshake_client_sessions: HashMap::new(),
        }
    }

    fn allow_initiate_handshake(
        &self,
        remote_public_key: &PublicKey,
    ) -> Result<(), HandshakeError> {
        match self.neighbors.borrow().get(remote_public_key) {
            None => return Err(HandshakeError::UnknownNeighbor),
            Some(neighbor) => {
                if neighbor.remote_addr().is_none() {
                    return Err(HandshakeError::LocalhostNotInitiator);
                }
            }
        }

        let requesting_nonce = self
            .request_nonce_sessions
            .values()
            .any(|s| *s.remote_public_key() == *remote_public_key);

        if requesting_nonce
            || self
                .public_key_to_hash_result
                .contains_key(remote_public_key)
        {
            return Err(HandshakeError::HandshakeInProgress);
        }

        Ok(())
    }

    pub fn initiate_handshake(
        &mut self,
        remote_public_key: PublicKey,
    ) -> Result<RequestNonce, HandshakeError> {
        self.allow_initiate_handshake(&remote_public_key)?;

        let request_nonce = RequestNonce {
            request_rand_nonce: loop {
                // Generate a nonce which is not in use
                let new_rand_value = RandValue::new(&*self.secure_rng);
                if !self.request_nonce_sessions.contains_key(&new_rand_value) {
                    break new_rand_value;
                }
            },
        };

        self.request_nonce_sessions.insert(
            request_nonce.request_rand_nonce.clone(),
            RequestNonceSession::new(remote_public_key),
        );

        Ok(request_nonce)
    }

    fn check_response_nonce(&self, response_nonce: &ResponseNonce) -> Result<(), HandshakeError> {
        let remote_public_key = self
            .request_nonce_sessions
            .get(&response_nonce.request_rand_nonce)
            .ok_or(HandshakeError::RequestNonceSessionNotFound)
            .map(|session| session.remote_public_key())?;

        if verify_signature(
            &response_nonce.as_bytes(),
            remote_public_key,
            &response_nonce.signature,
        ) {
            Ok(())
        } else {
            Err(HandshakeError::SignatureVerificationFailed)
        }
    }

    pub fn handle_response_nonce(
        &mut self,
        response_nonce: ResponseNonce,
    ) -> Result<ExchangeActive, HandshakeError> {
        self.check_response_nonce(&response_nonce)?;

        let remote_public_key = self
            .request_nonce_sessions
            .remove(&response_nonce.request_rand_nonce)
            .expect("invalid response nonce message")
            .finish();

        let key_salt = Salt::new(&*self.secure_rng).map_err(HandshakeError::CryptoError)?;
        let local_dh_private_key =
            DhPrivateKey::new(&*self.secure_rng).map_err(HandshakeError::CryptoError)?;
        let local_dh_public_key = local_dh_private_key
            .compute_public_key()
            .map_err(HandshakeError::CryptoError)?;

        let exchange_active = ExchangeActive {
            key_salt,
            dh_public_key: local_dh_public_key,
            responder_rand_nonce: response_nonce.responder_rand_nonce,
            initiator_rand_nonce: RandValue::new(&*self.secure_rng),
            initiator_public_key: self.local_public_key.clone(),
            responder_public_key: remote_public_key.clone(),
            signature: Signature::from(&[0x00; SIGNATURE_LEN]),
        };

        let new_session = HandshakeClientSession {
            remote_public_key: remote_public_key.clone(),

            local_dh_private_key,
            sent_key_salt: exchange_active.key_salt.clone(),
            recv_rand_nonce: exchange_active.responder_rand_nonce.clone(),
            sent_rand_nonce: exchange_active.initiator_rand_nonce.clone(),

            timeout: HANDSHAKE_SESSION_TIMEOUT,
        };
        let last_hash = sha_512_256(&exchange_active.as_bytes());

        match self
            .handshake_client_sessions
            .insert(last_hash.clone(), new_session)
        {
            None => match self
                .public_key_to_hash_result
                .insert(remote_public_key, last_hash)
            {
                None => Ok(exchange_active),
                Some(_) => panic!("sessions index inconsistent"),
            },
            Some(_) => Err(HandshakeError::SessionAlreadyExists),
        }
    }

    fn check_exchange_passive(
        &self,
        exchange_passive: &ExchangePassive,
    ) -> Result<(), HandshakeError> {
        let remote_public_key_ref = self
            .handshake_client_sessions
            .get(&exchange_passive.prev_hash)
            .ok_or(HandshakeError::HandshakeSessionNotFound)
            .and_then(|session| Ok(session.remote_public_key()))?;

        if verify_signature(
            &exchange_passive.as_bytes(),
            remote_public_key_ref,
            &exchange_passive.signature,
        ) {
            Ok(())
        } else {
            Err(HandshakeError::SignatureVerificationFailed)
        }
    }

    pub fn handle_exchange_passive(
        &mut self,
        exchange_passive: ExchangePassive,
    ) -> Result<(ChannelMetadata, ChannelReady), HandshakeError> {
        self.check_exchange_passive(&exchange_passive)?;

        let session = self
            .handshake_client_sessions
            .remove(&exchange_passive.prev_hash)
            .expect("invalid exchange passive message");
        self.public_key_to_hash_result
            .remove(session.remote_public_key());

        let channel_ready = ChannelReady {
            prev_hash: sha_512_256(&exchange_passive.as_bytes()),
            signature: Signature::from(&[0x00; SIGNATURE_LEN]),
        };

        let channel_metadata =
            session.finish(exchange_passive.key_salt, exchange_passive.dh_public_key)?;

        Ok((channel_metadata, channel_ready))
    }

    pub fn remove_session_by_public_key(&mut self, public_key: &PublicKey) {
        if let Some(last_hash) = self.public_key_to_hash_result.remove(public_key) {
            self.handshake_client_sessions.remove(&last_hash);
        }
    }

    pub fn time_tick(&mut self) {
        self.request_nonce_sessions.retain(|_, session| {
            if session.timeout >= 1 {
                session.timeout -= 1;
                true
            } else {
                false
            }
        });

        let mut expired = Vec::new();
        self.handshake_client_sessions.retain(|_, session| {
            if session.timeout >= 1 {
                session.timeout -= 1;
                true
            } else {
                expired.push(session.remote_public_key().clone());
                false
            }
        });
        for public_key in expired {
            self.public_key_to_hash_result.remove(&public_key);
        }
    }
}
