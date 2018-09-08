use std::{cell::RefCell, collections::HashMap, rc::Rc};

use ring::rand::SecureRandom;

use crypto::{
    dh::{DhPrivateKey, DhPublicKey, Salt},
    hash::{sha_512_256, HashResult},
    identity::{verify_signature, PublicKey, Signature, SIGNATURE_LEN},
    rand_values::{RandValue, RandValuesStore},
};

use proto::channeler::*;

use super::{
    super::{
        config::{HANDSHAKE_SESSION_TIMEOUT, RAND_VALUES_STORE_CAPACITY, RAND_VALUES_STORE_TICKS},
        types::NeighborTable,
    },
    helpers::finish_handshake,
    ChannelMetadata,
    HandshakeError,
};

pub struct HandshakeServer<SR> {
    // TODO: Shoule we remove this field, we not use this in verifying exchange active message for
    // now.
    local_public_key: PublicKey,
    neighbors:        Rc<RefCell<NeighborTable>>,
    secure_rng:       Rc<SR>,

    rand_values_store:         RandValuesStore,
    handshake_server_sessions: HashMap<HashResult, HandshakeServerSession>,
    public_key_to_hash_result: HashMap<PublicKey, HashResult>,
}

struct HandshakeServerSession {
    remote_public_key: PublicKey,

    recv_rand_nonce:      RandValue,
    sent_rand_nonce:      RandValue,
    sent_key_salt:        Salt,
    recv_key_salt:        Salt,
    local_dh_private_key: DhPrivateKey,
    remote_dh_public_key: DhPublicKey,

    timeout: usize,
}

impl HandshakeServerSession {
    #[inline]
    pub fn remote_public_key(&self) -> &PublicKey {
        &self.remote_public_key
    }

    pub fn finish(self) -> Result<ChannelMetadata, HandshakeError> {
        finish_handshake(
            self.remote_public_key,
            self.sent_rand_nonce,
            self.recv_rand_nonce,
            self.sent_key_salt,
            self.recv_key_salt,
            self.remote_dh_public_key,
            self.local_dh_private_key,
        )
    }
}

impl<SR: SecureRandom> HandshakeServer<SR> {
    pub fn new(
        local_public_key: PublicKey,
        neighbors: Rc<RefCell<NeighborTable>>,
        rng: Rc<SR>,
    ) -> HandshakeServer<SR> {
        let rand_values_store =
            RandValuesStore::new(&*rng, RAND_VALUES_STORE_TICKS, RAND_VALUES_STORE_CAPACITY);
        HandshakeServer {
            local_public_key,

            neighbors,
            secure_rng: rng,

            rand_values_store,
            handshake_server_sessions: HashMap::new(),
            public_key_to_hash_result: HashMap::new(),
        }
    }

    pub fn handle_request_nonce(
        &self,
        request_nonce: RequestNonce,
    ) -> Result<ResponseNonce, HandshakeError> {
        let response_nonce = ResponseNonce {
            request_rand_nonce:  request_nonce.request_rand_nonce,
            response_rand_nonce: RandValue::new(&*self.secure_rng),
            // XXX: The `last_rand_value` just make a copy, can we it here?
            responder_rand_nonce: self.rand_values_store.last_rand_value(),
            signature:            Signature::from(&[0x00; SIGNATURE_LEN]),
        };

        Ok(response_nonce)
    }

    fn check_exchange_active(
        &self,
        exchange_active: &ExchangeActive,
    ) -> Result<(), HandshakeError> {
        let remote_public_key_ref = &exchange_active.initiator_public_key;

        match self.neighbors.borrow().get(remote_public_key_ref) {
            Some(neighbor) => {
                if neighbor.remote_addr().is_some() {
                    return Err(HandshakeError::LocalhostNotResponder);
                }
            }
            None => return Err(HandshakeError::UnknownNeighbor),
        }

        if !verify_signature(
            &exchange_active.as_bytes(),
            remote_public_key_ref,
            &exchange_active.signature,
        ) {
            return Err(HandshakeError::SignatureVerificationFailed);
        }

        if !self
            .rand_values_store
            .contains(&exchange_active.responder_rand_nonce)
        {
            return Err(HandshakeError::InvalidResponderNonce);
        }

        if self
            .public_key_to_hash_result
            .contains_key(remote_public_key_ref)
        {
            return Err(HandshakeError::HandshakeInProgress);
        }

        Ok(())
    }

    pub fn handle_exchange_active(
        &mut self,
        exchange_active: ExchangeActive,
    ) -> Result<ExchangePassive, HandshakeError> {
        self.check_exchange_active(&exchange_active)?;

        let key_salt = Salt::new(&*self.secure_rng).map_err(HandshakeError::CryptoError)?;
        let local_dh_private_key =
            DhPrivateKey::new(&*self.secure_rng).map_err(HandshakeError::CryptoError)?;
        let local_dh_public_key = local_dh_private_key
            .compute_public_key()
            .map_err(HandshakeError::CryptoError)?;

        let exchange_passive = ExchangePassive {
            prev_hash: sha_512_256(&exchange_active.as_bytes()),
            dh_public_key: local_dh_public_key,
            key_salt,
            signature: Signature::from(&[0x00; SIGNATURE_LEN]),
        };
        let remote_public_key = exchange_active.initiator_public_key;

        let new_session = HandshakeServerSession {
            remote_public_key: remote_public_key.clone(),
            local_dh_private_key,
            sent_key_salt: exchange_passive.key_salt.clone(),
            recv_key_salt: exchange_active.key_salt,
            sent_rand_nonce: exchange_active.responder_rand_nonce,
            recv_rand_nonce: exchange_active.initiator_rand_nonce,
            remote_dh_public_key: exchange_active.dh_public_key,

            timeout: HANDSHAKE_SESSION_TIMEOUT,
        };
        let last_hash = sha_512_256(&exchange_passive.as_bytes());

        match self
            .handshake_server_sessions
            .insert(last_hash.clone(), new_session)
        {
            None => match self
                .public_key_to_hash_result
                .insert(remote_public_key, last_hash)
            {
                None => Ok(exchange_passive),
                Some(_) => panic!("public key to hash index error"),
            },
            Some(_) => Err(HandshakeError::HandshakeInProgress),
        }
    }

    fn check_channel_ready(&self, channel_ready: &ChannelReady) -> Result<(), HandshakeError> {
        let remote_public_key_ref = self
            .handshake_server_sessions
            .get(&channel_ready.prev_hash)
            .ok_or(HandshakeError::HandshakeSessionNotFound)
            .and_then(|session| Ok(session.remote_public_key()))?;

        if verify_signature(
            &channel_ready.as_bytes(),
            remote_public_key_ref,
            &channel_ready.signature,
        ) {
            Ok(())
        } else {
            Err(HandshakeError::SignatureVerificationFailed)
        }
    }

    pub fn handle_channel_ready(
        &mut self,
        channel_ready: ChannelReady,
    ) -> Result<ChannelMetadata, HandshakeError> {
        self.check_channel_ready(&channel_ready)?;

        let session = self
            .handshake_server_sessions
            .remove(&channel_ready.prev_hash)
            .expect("invalid channel ready message");

        session.finish()
    }

    pub fn remove_session_by_public_key(&mut self, public_key: &PublicKey) {
        if let Some(last_hash) = self.public_key_to_hash_result.remove(public_key) {
            self.handshake_server_sessions.remove(&last_hash);
        }
    }

    pub fn time_tick(&mut self) {
        self.rand_values_store.time_tick(&*self.secure_rng);

        let mut expired = Vec::new();
        self.handshake_server_sessions.retain(|_, session| {
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
