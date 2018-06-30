use std::rc::Rc;
use std::cell::RefCell;
use std::collections::HashMap;

use ring::rand::SecureRandom;
use ring::aead::{SealingKey, OpeningKey};

use crypto::hash::sha_512_256;
use crypto::dh::{DhPrivateKey, Salt};
use crypto::rand_values::{RandValue, RandValuesStore};
use crypto::identity::{PublicKey, Signature, SIGNATURE_LEN, verify_signature};
use channeler::config::{RAND_VALUES_STORE_TICKS, RAND_VALUES_STORE_CAPACITY};
use proto::channeler::*;

use super::NeighborTable;

mod state;
mod error;
mod session;

pub use self::error::Error;
pub use self::state::HandshakeState;
pub use self::session::{SessionId, SessionTable, RequestNonceSession, HandshakeSession};

pub struct ChannelMetadata {
    pub remote_public_key: PublicKey,

    pub tx_cid: ChannelId,
    pub tx_key: SealingKey,

    pub rx_cid: ChannelId,
    pub rx_key: OpeningKey,
}

pub struct Handshaker<SR> {
    local_public_key: PublicKey,
    neighbors: Rc<RefCell<NeighborTable>>,

    secure_rng: Rc<SR>,
    rand_values_store: RandValuesStore,

    request_nonce_sessions: HashMap<RandValue, RequestNonceSession>,
    handshake_sessions: SessionTable<HandshakeState>,
}

impl<R: SecureRandom> Handshaker<R> {
    pub fn new(local_public_key: PublicKey, neighbors: Rc<RefCell<NeighborTable>>, rng: Rc<R>) -> Handshaker<R> {
        let rand_values_store = RandValuesStore::new(&*rng, RAND_VALUES_STORE_TICKS, RAND_VALUES_STORE_CAPACITY);

        Handshaker {
            neighbors,
            local_public_key,
            secure_rng: rng,
            rand_values_store,
            handshake_sessions: SessionTable::new(),
            request_nonce_sessions: HashMap::new(),
        }
    }

    pub fn initiate_handshake(&mut self, remote_public_key: PublicKey) -> Result<RequestNonce, Error> {
        match self.neighbors.borrow().get(&remote_public_key) {
            Some(neighbor) => {
                if !neighbor.remote_addr().is_some() {
                    return Err(Error::LocalhostNotInitiator);
                }
            },
            None => return Err(Error::UnknownNeighbor),
        }

        for session in self.request_nonce_sessions.values() {
            if *session.remote_public_key() == remote_public_key {
                return Err(Error::HandshakeInProgress)
            }
        }

        let sid = SessionId::new_initiator(remote_public_key.clone());
        if self.handshake_sessions.contains_session(&sid) {
            return Err(Error::HandshakeInProgress);
        }

        let request_nonce = RequestNonce {
            request_rand_nonce: loop {
                // Generate a nonce which does not in use
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

    pub fn handle_request_nonce(&self, request_nonce: RequestNonce) -> Result<ResponseNonce, Error> {
        let response_nonce = ResponseNonce {
            request_rand_nonce: request_nonce.request_rand_nonce,
            response_rand_nonce: RandValue::new(&*self.secure_rng),
            // XXX: The `last_rand_value` just make a copy, can we it here?
            responder_rand_nonce: self.rand_values_store.last_rand_value(),
            signature: Signature::from(&[0x00; SIGNATURE_LEN]),
        };

        Ok(response_nonce)
    }

    fn check_response_nonce(&self, response_nonce: &ResponseNonce) -> Result<(), Error> {
        let remote_public_key_ref = self.request_nonce_sessions
            .get(&response_nonce.request_rand_nonce)
            .ok_or(Error::RequestNonceSessionNotFound)
            .map(|session| session.remote_public_key())?;

        if verify_signature(&response_nonce.as_bytes(), remote_public_key_ref, &response_nonce.signature) {
            Ok(())
        } else {
            Err(Error::SignatureVerificationFailed)
        }
    }

    pub fn handle_response_nonce(&mut self, response_nonce: ResponseNonce) -> Result<ExchangeActive, Error> {
        self.check_response_nonce(&response_nonce)?;

        let remote_public_key = self.request_nonce_sessions
            .remove(&response_nonce.request_rand_nonce)
            .expect("access controller error: invalid response nonce message")
            .finish();

        let key_salt = Salt::new(&*self.secure_rng).map_err(Error::CryptoError)?;
        let local_dh_private_key = DhPrivateKey::new(&*self.secure_rng).map_err(Error::CryptoError)?;
        let local_dh_public_key = local_dh_private_key.compute_public_key().map_err(Error::CryptoError)?;

        let exchange_active = ExchangeActive {
            key_salt,
            dh_public_key: local_dh_public_key,
            responder_rand_nonce: response_nonce.responder_rand_nonce,
            initiator_rand_nonce: RandValue::new(&*self.secure_rng),
            initiator_public_key: self.local_public_key.clone(),
            responder_public_key: remote_public_key.clone(),
            signature: Signature::from(&[0x00; SIGNATURE_LEN]),
        };

        let sid = SessionId::new_initiator(remote_public_key.clone());
        let state = HandshakeState::AfterInitiatorExchangeActive {
            local_dh_private_key,
            sent_key_salt: exchange_active.key_salt.clone(),
            recv_rand_nonce: exchange_active.responder_rand_nonce.clone(),
            sent_rand_nonce: exchange_active.initiator_rand_nonce.clone(),
        };
        let last_hash = sha_512_256(&exchange_active.as_bytes());
        let new_session = HandshakeSession::new(sid, state, last_hash);

        match self.handshake_sessions.add_session(new_session) {
            None => Ok(exchange_active),
            Some(_) => Err(Error::SessionAlreadyExists)
        }
    }

    fn check_exchange_active(&self, exchange_active: &ExchangeActive) -> Result<(), Error> {
        let remote_public_key_ref = &exchange_active.initiator_public_key;

        match self.neighbors.borrow().get(remote_public_key_ref) {
            Some(neighbor) => {
                if neighbor.remote_addr().is_some() {
                    return Err(Error::LocalhostNotResponder);
                }
            },
            None => return Err(Error::UnknownNeighbor),
        }

        if !verify_signature(&exchange_active.as_bytes(), remote_public_key_ref, &exchange_active.signature) {
            return Err(Error::SignatureVerificationFailed)
        }

        if !self.rand_values_store.contains(&exchange_active.responder_rand_nonce) {
            return Err(Error::InvalidResponderNonce);
        }

        let sid = SessionId::new_responder(remote_public_key_ref.clone());
        if self.handshake_sessions.contains_session(&sid) {
            return Err(Error::SessionAlreadyExists);
        }

        Ok(())
    }

    pub fn handle_exchange_active(&mut self, exchange_active: ExchangeActive) -> Result<ExchangePassive, Error> {
        self.check_exchange_active(&exchange_active)?;

        let key_salt = Salt::new(&*self.secure_rng).map_err(Error::CryptoError)?;
        let local_dh_private_key = DhPrivateKey::new(&*self.secure_rng).map_err(Error::CryptoError)?;
        let local_dh_public_key = local_dh_private_key.compute_public_key().map_err(Error::CryptoError)?;

        let exchange_passive = ExchangePassive {
            prev_hash: sha_512_256(&exchange_active.as_bytes()),
            dh_public_key: local_dh_public_key,
            key_salt,
            signature: Signature::from(&[0x00; SIGNATURE_LEN]),
        };

        let sid = SessionId::new_responder(exchange_active.initiator_public_key);
        let state = HandshakeState::AfterResponderExchangePassive {
            local_dh_private_key,
            sent_key_salt: exchange_passive.key_salt.clone(),
            recv_key_salt: exchange_active.key_salt,
            sent_rand_nonce: exchange_active.responder_rand_nonce,
            recv_rand_nonce: exchange_active.initiator_rand_nonce,
            remote_dh_public_key: exchange_active.dh_public_key,
        };
        let last_hash = sha_512_256(&exchange_passive.as_bytes());
        let new_session = HandshakeSession::new(sid, state, last_hash);

        match self.handshake_sessions.add_session(new_session) {
            None => Ok(exchange_passive),
            Some(_) => Err(Error::SessionAlreadyExists),
        }
    }

    fn check_exchange_passive(&self, exchange_passive: &ExchangePassive) -> Result<(), Error> {
        let remote_public_key_ref = self.handshake_sessions
            .get_session(&exchange_passive.prev_hash)
            .ok_or(Error::HandshakeSessionNotFound)
            .and_then(|session| {
                if session.is_after_initiator_exchange_active() {
                    Ok(session.remote_public_key())
                } else {
                    Err(Error::InconsistentState)
                }
            })?;

        if verify_signature(&exchange_passive.as_bytes(), remote_public_key_ref,&exchange_passive.signature) {
            Ok(())
        } else {
            Err(Error::SignatureVerificationFailed)
        }
    }

    pub fn handle_exchange_passive(&mut self, exchange_passive: ExchangePassive) -> Result<(ChannelMetadata, ChannelReady), Error> {
        self.check_exchange_passive(&exchange_passive)?;

        let session = self.handshake_sessions
            .remove_session_by_hash(&exchange_passive.prev_hash)
            .expect("access controller error: invalid exchange passive message");

        let channel_ready = ChannelReady {
            prev_hash: sha_512_256(&exchange_passive.as_bytes()),
            signature: Signature::from(&[0x00; SIGNATURE_LEN]),
        };

        let channel_metadata = session.initiator_finish(exchange_passive.key_salt, exchange_passive.dh_public_key)?;

        Ok((channel_metadata, channel_ready))
    }

    fn check_channel_ready(&self, channel_ready: &ChannelReady) -> Result<(), Error> {
        let remote_public_key_ref = self.handshake_sessions
            .get_session(&channel_ready.prev_hash)
            .ok_or(Error::HandshakeSessionNotFound)
            .and_then(|session| {
                if session.is_after_responder_exchange_passive() {
                    Ok(session.remote_public_key())
                } else {
                    Err(Error::InconsistentState)
                }
            })?;

        if verify_signature(&channel_ready.as_bytes(), remote_public_key_ref, &channel_ready.signature) {
            Ok(())
        } else {
            Err(Error::SignatureVerificationFailed)
        }
    }

    pub fn handle_channel_ready(&mut self, channel_ready: ChannelReady) -> Result<ChannelMetadata, Error> {
        self.check_channel_ready(&channel_ready)?;

        let session = self.handshake_sessions
            .remove_session_by_hash(&channel_ready.prev_hash)
            .expect("access controller error: invalid channel ready message");

        session.responder_finish()
    }

    pub fn time_tick(&mut self) {
        self.rand_values_store.time_tick(&*self.secure_rng);

        self.request_nonce_sessions.retain(|_, request_nonce_session| {
            if request_nonce_session.timeout <= 1 {
                false
            } else {
                request_nonce_session.timeout -= 1;
                true
            }
        });

        self.handshake_sessions.time_tick();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::collections::HashMap;

    use crypto::identity::Identity;
    use crypto::identity::SoftwareEd25519Identity;

    use channeler::types::{ChannelerNeighborInfo, ChannelerNeighbor};
    use ring::rand::SystemRandom;
    use ring::signature::Ed25519KeyPair;
    use ring::test::rand::FixedByteRandom;

    // TODO: Add a macro to construct test.

    #[test]
    fn handshake_happy_path() {
        let (public_key_a, identity_a) = {
            let fixed_rand = FixedByteRandom { byte: 0x00 };
            let pkcs8 = Ed25519KeyPair::generate_pkcs8(&fixed_rand).unwrap();
            let identity = SoftwareEd25519Identity::from_pkcs8(&pkcs8).unwrap();
            let public_key = identity.get_public_key();

            (public_key, identity)
        };

        let (public_key_b, identity_b) = {
            let fixed_rand = FixedByteRandom { byte: 0x01 };
            let pkcs8 = Ed25519KeyPair::generate_pkcs8(&fixed_rand).unwrap();
            let identity = SoftwareEd25519Identity::from_pkcs8(&pkcs8).unwrap();
            let public_key = identity.get_public_key();

            (public_key, identity)
        };

        let info_a = ChannelerNeighborInfo {
            public_key: public_key_a.clone(),
            socket_addr: None,
        };
        let info_b = ChannelerNeighborInfo {
            public_key: public_key_b.clone(),
            socket_addr: Some("127.0.0.1:10001".parse().unwrap()),
        };

        let node_a = ChannelerNeighbor::new(info_a);
        let node_b = ChannelerNeighbor::new(info_b);

        let (neighbors_a, neighbors_b) = {
            let mut neighbors_a = HashMap::new();
            neighbors_a.insert(node_b.remote_public_key().clone(), node_b);

            let mut neighbors_b = HashMap::new();
            neighbors_b.insert(node_a.remote_public_key().clone(), node_a);

            (
                Rc::new(RefCell::new(neighbors_a)),
                Rc::new(RefCell::new(neighbors_b)),
            )
        };

        let shared_secure_rng = Rc::new(SystemRandom::new());

        let mut hs_state_machine_a = Handshaker::new(
            public_key_a.clone(),
            neighbors_a,
            Rc::clone(&shared_secure_rng),
        );

        let mut hs_state_machine_b = Handshaker::new(
            public_key_b.clone(),
            neighbors_b,
            Rc::clone(&shared_secure_rng),
        );

        // A -> B: RequestNonce
        let request_nonce_to_b = hs_state_machine_a.initiate_handshake(
            public_key_b.clone()
        ).unwrap();

        // B -> A: RespondNonce
        let mut respond_nonce_to_a = hs_state_machine_b.handle_request_nonce(
            request_nonce_to_b
        ).unwrap();
        respond_nonce_to_a.signature = identity_b.sign_message(
            &respond_nonce_to_a.as_bytes()
        );

        // A -> B: ExchangeActive
        let mut exchange_active_to_b = hs_state_machine_a.handle_response_nonce(
            respond_nonce_to_a
        ).unwrap();
        exchange_active_to_b.signature = identity_a.sign_message(
            &exchange_active_to_b.as_bytes()
        );

        // B -> A: ExchangePassive
        let mut exchange_passive_to_a = hs_state_machine_b.handle_exchange_active(
            exchange_active_to_b
        ).unwrap();
        exchange_passive_to_a.signature = identity_b.sign_message(
            &exchange_passive_to_a.as_bytes()
        );

        // A -> B: ChannelReady (A: Finish)
        let (new_channel_info_a, mut channel_ready_to_b) = hs_state_machine_a.handle_exchange_passive(
            exchange_passive_to_a
        ).unwrap();
        channel_ready_to_b.signature = identity_a.sign_message(
            &channel_ready_to_b.as_bytes()
        );

        // B: Finish
        let new_channel_info_b = hs_state_machine_b.handle_channel_ready(
            channel_ready_to_b
        ).unwrap();

        assert_eq!(new_channel_info_a.tx_cid, new_channel_info_b.rx_cid);
        assert_eq!(new_channel_info_b.tx_cid, new_channel_info_a.rx_cid);
    }
}
