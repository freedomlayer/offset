use std::rc::Rc;
use std::cell::RefCell;

use ring::rand::SecureRandom;
use bytes::Bytes;

use crypto::dh::{DhPrivateKey, DhPublicKey, Salt};
use crypto::identity::{PublicKey, verify_signature, SIGNATURE_LEN, Signature};
use crypto::hash::{HashResult, sha_512_256};
use crypto::rand_values::{RandValue, RandValuesStore};
use proto::channeler::{RequestNonce, RespondNonce, ExchangeActive, ExchangePassive, ChannelReady};
use channeler::types::NeighborsTable;

use super::Result;
use super::error::HandshakeError;
use super::types::*;

pub enum HandshakeState {
    InitiatorRequestNonce,

    InitiatorExchangeActive {
        recv_rand_nonce: RandValue,
        sent_rand_nonce: RandValue,

        sent_key_salt: Salt,

        my_private_key: DhPrivateKey,
    },

    ExchangePassive {
        sent_rand_nonce: RandValue,
        recv_rand_nonce: RandValue,

        sent_key_salt: Salt,
        recv_key_salt: Salt,

        remote_dh_public_key: DhPublicKey,

        my_private_key: DhPrivateKey,
    }
}

impl HandshakeState {
    pub fn request_nonce() -> HandshakeState {
        HandshakeState::InitiatorRequestNonce
    }

    pub fn transfer2passive(self, recv_key_salt: Salt, remote_dh_public_key: DhPublicKey) -> Result<HandshakeState> {
        match self {
            HandshakeState::InitiatorExchangeActive {
                sent_rand_nonce,
                recv_rand_nonce,
                sent_key_salt,
                my_private_key,
            } => {
                Ok(HandshakeState::ExchangePassive {
                    sent_rand_nonce,
                    recv_rand_nonce,
                    sent_key_salt,
                    recv_key_salt,
                    remote_dh_public_key,
                    my_private_key,
                })
            }
            _ => Err(HandshakeError::InvalidTransfer),
        }
    }
}

pub struct Handshaker<SR> {
    neighbors: Rc<RefCell<NeighborsTable>>,
    secure_rng: Rc<SR>,
    sessions_map: HandshakeSessionMap,
    rand_values_store: RandValuesStore,
    my_public_key: PublicKey,

    handshake_timeout: usize,
}

impl<SR: SecureRandom> Handshaker<SR> {
    pub fn new(
        neighbors: Rc<RefCell<NeighborsTable>>,
        secure_rng: Rc<SR>,
        my_public_key: PublicKey,
        // TODO: HandshakeConfig
    ) -> Handshaker<SR> {
        let rand_values_store = RandValuesStore::new(&*secure_rng, 5, 3);

        Handshaker {
            neighbors,
            secure_rng,
            rand_values_store,
            my_public_key,
            sessions_map: HandshakeSessionMap::new(),
            handshake_timeout: 100,
        }
    }

    pub fn new_handshake(&mut self, remote_public_key: PublicKey) -> Result<RequestNonce> {
        let id = HandshakeId::new(HandshakeRole::Initiator, remote_public_key);

        if self.sessions_map.contains_id(&id) {
            return Err(HandshakeError::SessionExist);
        }

        let request_nonce = RequestNonce {
            rand_nonce: RandValue::new(&*self.secure_rng),
        };

        let state = HandshakeState::request_nonce();
        let session = HandshakeSession::new(id, state, self.handshake_timeout);

        let last_hash = sha_512_256(&request_nonce.as_bytes());
        match self.sessions_map.insert(last_hash, session) {
            None => Ok(request_nonce),
            Some(_) => Err(HandshakeError::SessionExist),
        }
    }

    pub fn process_request_nonce(&mut self, request_nonce: RequestNonce) -> Result<RespondNonce> {
        let respond_nonce = RespondNonce {
            req_rand_nonce: request_nonce.rand_nonce,
            res_rand_nonce: RandValue::new(&*self.secure_rng),
            responder_rand_nonce: self.rand_values_store.last_rand_value(),

            signature: Signature::from(&[0x00; SIGNATURE_LEN]),
        };

        Ok(respond_nonce)
    }

    pub fn process_respond_nonce(&mut self, respond_nonce: RespondNonce) -> Result<ExchangeActive> {
        // Check whether we sent the RequestNonce message
        let prev_hash = sha_512_256(&respond_nonce.req_rand_nonce);
        let mut session = self.sessions_map.take_by_hash(&prev_hash)
            .ok_or(HandshakeError::NoSuchSession)?;

        match session.state() {
            HandshakeState::InitiatorRequestNonce => (),
            _ => return Err(HandshakeError::InvalidTransfer)
        }

        // Verify the signature of this message
        let msg = respond_nonce.as_bytes();
        if !verify_signature(&msg, session.remote_public_key(), &respond_nonce.signature) {
            return Err(HandshakeError::InvalidSignature);
        }

        // Generate DH private key and key salt
        let my_private_key = DhPrivateKey::new(&*self.secure_rng)
            .map_err(HandshakeError::Crypto)?;
        let dh_public_key = my_private_key.compute_public_key()
            .map_err(HandshakeError::Crypto)?;
        let key_salt = Salt::new(&*self.secure_rng)
            .map_err(HandshakeError::Crypto)?;

        let exchange_active = ExchangeActive {
            key_salt,
            dh_public_key,

            responder_rand_nonce: respond_nonce.responder_rand_nonce,
            initiator_rand_nonce: RandValue::new(&*self.secure_rng),
            initiator_public_key: self.my_public_key.clone(),
            responder_public_key: session.remote_public_key().clone(),
            signature: Signature::from(&[0x00; SIGNATURE_LEN]),
        };

        *session.state_mut() = HandshakeState::InitiatorExchangeActive {
            sent_rand_nonce: exchange_active.initiator_rand_nonce.clone(),
            recv_rand_nonce: exchange_active.responder_rand_nonce.clone(),
            sent_key_salt: exchange_active.key_salt.clone(),
            my_private_key,
        };

        let last_hash = sha_512_256(&exchange_active.as_bytes());
        match self.sessions_map.insert(last_hash, session) {
            None => Ok(exchange_active),
            Some(_) => Err(HandshakeError::SessionExist),
        }
    }

    pub fn process_exchange_active(&mut self, exchange_active: ExchangeActive) -> Result<ExchangePassive> {
        let remote_public_key = &exchange_active.initiator_public_key;
        let id = HandshakeId::new(HandshakeRole::Responder, remote_public_key.clone());

        match self.neighbors.borrow().get(remote_public_key) {
            Some(neighbor) => {
                // Check whether a allowed initiator
                if neighbor.info.socket_addr.is_some() {
                    return Err(HandshakeError::NotAllowed)
                } else {
                    // Check whether an ongoing handshake session with initiator
                    if self.sessions_map.contains_id(&id) {
                        return Err(HandshakeError::SessionExist)
                    }
                }
            },
            None => return Err(HandshakeError::NotAllowed),
        }

        // Verify the signature of this message
        let msg = exchange_active.as_bytes();
        if !verify_signature(&msg, remote_public_key, &exchange_active.signature) {
            return Err(HandshakeError::InvalidSignature)
        }

        // Generate DH private key and key salt
        let my_private_key = DhPrivateKey::new(&*self.secure_rng)
            .map_err(HandshakeError::Crypto)?;
        let dh_public_key = my_private_key.compute_public_key()
            .map_err(HandshakeError::Crypto)?;
        let key_salt = Salt::new(&*self.secure_rng)
            .map_err(HandshakeError::Crypto)?;

        let exchange_passive = ExchangePassive {
            prev_hash: sha_512_256(&msg),
            dh_public_key,
            key_salt,

            signature: Signature::from(&[0x00; SIGNATURE_LEN]),
        };

        let state = HandshakeState::ExchangePassive {
            sent_key_salt: exchange_passive.key_salt.clone(),
            recv_key_salt: exchange_active.key_salt,
            sent_rand_nonce: exchange_active.responder_rand_nonce,
            recv_rand_nonce: exchange_active.initiator_rand_nonce,
            remote_dh_public_key: exchange_active.dh_public_key,
            my_private_key,
        };

        let session = HandshakeSession::new(id, state, self.handshake_timeout);

        let last_hash = sha_512_256(&exchange_passive.as_bytes());
        match self.sessions_map.insert(last_hash, session) {
            None => Ok(exchange_passive),
            Some(_) => Err(HandshakeError::SessionExist),
        }
    }

    pub fn process_exchange_passive(&mut self, exchange_passive: ExchangePassive) -> Result<(HandshakeResult, ChannelReady)> {
        let mut session = self.sessions_map.take_by_hash(&exchange_passive.prev_hash)
            .ok_or(HandshakeError::NoSuchSession)?;
        match session.state() {
            HandshakeState::InitiatorExchangeActive { .. } => (),
            _ => return Err(HandshakeError::InvalidTransfer)
        }

        let remote_public_key = session.remote_public_key();

        if !verify_signature(&exchange_passive.as_bytes(), remote_public_key, &exchange_passive.signature) {
            return Err(HandshakeError::InvalidSignature);
        }

        let channel_ready = ChannelReady {
            prev_hash: sha_512_256(&exchange_passive.as_bytes()),
            signature: Signature::from(&[0x00; SIGNATURE_LEN]),
        };

        session.state = session.state.transfer2passive(
            exchange_passive.key_salt,
            exchange_passive.dh_public_key
        )?;

        let new_channel_info = session.finish()?;

        Ok((new_channel_info, channel_ready))
    }

    pub fn process_channel_ready(&mut self, channel_ready: ChannelReady) -> Result<HandshakeResult> {
        let session = self.sessions_map.take_by_hash(&channel_ready.prev_hash)
            .ok_or(HandshakeError::NoSuchSession)?;

        let remote_public_key = session.remote_public_key();

        if !verify_signature(&channel_ready.prev_hash, remote_public_key, &channel_ready.signature) {
            return Err(HandshakeError::InvalidSignature);
        }

        session.finish()
    }

    pub fn time_tick(&mut self) {
        self.sessions_map.time_tick();
        self.rand_values_store.time_tick(&*self.secure_rng);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio_core::reactor::{Core, Timeout};
    use channeler::types::{ChannelerNeighborInfo, NeighborsTable, ChannelerNeighbor};
    use crypto::identity::{Identity, PublicKey};
    use crypto::identity::SoftwareEd25519Identity;
    use futures::prelude::*;
    use futures::sync::mpsc::channel;
    use ring::rand::SystemRandom;
    use ring::signature::Ed25519KeyPair;
    use ring::test::rand::FixedByteRandom;
    use security_module::client::SecurityModuleClient;
    use security_module::create_security_module;
    use std::collections::HashMap;

    const TICKS: usize = 100;

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

        let info_a = ChannelerNeighbor {
            info: ChannelerNeighborInfo {
                public_key: public_key_a.clone(),
                socket_addr: None,
            },
            retry_ticks: TICKS,
        };
        let info_b = ChannelerNeighbor {
            info: ChannelerNeighborInfo {
                public_key: public_key_b.clone(),
                socket_addr: Some("127.0.0.1:10001".parse().unwrap()),
            },
            retry_ticks: TICKS,
        };

        let (neighbors_a, neighbors_b) = {
            let mut neighbors_a = HashMap::new();
            neighbors_a.insert(info_b.info.public_key.clone(), info_b);

            let mut neighbors_b = HashMap::new();
            neighbors_b.insert(info_a.info.public_key.clone(), info_a);

            (
                Rc::new(RefCell::new(neighbors_a)),
                Rc::new(RefCell::new(neighbors_b)),
            )
        };

        let shared_secure_rng = Rc::new(SystemRandom::new());

        let mut hs_state_machine_a = Handshaker::new(
            neighbors_a,
            Rc::clone(&shared_secure_rng),
            public_key_a.clone(),
        );

        let mut hs_state_machine_b = Handshaker::new(
            neighbors_b,
            Rc::clone(&shared_secure_rng),
            public_key_b.clone(),
        );

        // A -> B: RequestNonce
        let request_nonce_to_b = hs_state_machine_a.new_handshake(
            public_key_b.clone()
        ).unwrap();

        // B -> A: RespondNonce
        let mut respond_nonce_to_a = hs_state_machine_b.process_request_nonce(
            request_nonce_to_b
        ).unwrap();
        respond_nonce_to_a.signature = identity_b.sign_message(
            &respond_nonce_to_a.as_bytes()
        );

        // A -> B: ExchangeActive
        let mut exchange_active_to_b = hs_state_machine_a.process_respond_nonce(
            respond_nonce_to_a
        ).unwrap();
        exchange_active_to_b.signature = identity_a.sign_message(
            &exchange_active_to_b.as_bytes()
        );

        // B -> A: ExchangePassive
        let mut exchange_passive_to_a = hs_state_machine_b.process_exchange_active(
            exchange_active_to_b
        ).unwrap();
        exchange_passive_to_a.signature = identity_b.sign_message(
            &exchange_passive_to_a.as_bytes()
        );

        // A -> B: ChannelReady (A: Finish)
        let (new_channel_info_a, mut channel_ready_to_b) = hs_state_machine_a.process_exchange_passive(
            exchange_passive_to_a
        ).unwrap();
        channel_ready_to_b.signature = identity_a.sign_message(
            &channel_ready_to_b.as_bytes()
        );

        // B: Finish
        let new_channel_info_b = hs_state_machine_b.process_channel_ready(
            channel_ready_to_b
        ).unwrap();

        assert_eq!(new_channel_info_a.sender_id, new_channel_info_b.receiver_id);
        assert_eq!(new_channel_info_b.receiver_id, new_channel_info_a.sender_id);
    }
}
