use std::rc::Rc;
use std::cell::RefCell;
use std::collections::HashMap;

use ring::rand::SecureRandom;
use ring::aead::{SealingKey, OpeningKey};

use crypto::hash::sha_512_256;
use crypto::dh::{DhPrivateKey, Salt};
use crypto::rand_values::{RandValue, RandValuesStore};
use crypto::identity::{PublicKey, Signature, SIGNATURE_LEN, verify_signature};

use proto::channeler::*;
use super::NeighborTable;

mod state;
mod error;
mod session;

pub use self::error::HandshakeError;
pub use self::state::HandshakeState;
pub use self::session::{SessionId, HandshakeSession};
pub use self::session::session_table::SessionTable;

const REQUEST_NONCE_TIMEOUT: usize = 100;

const RAND_VALUE_TICKS: usize = 5;
const RAND_VALUE_STORE_CAPACITY: usize = 3;

pub struct HandshakeResult {
    pub remote_public_key: PublicKey,

    pub sending_channel_id: ChannelId,
    pub sending_channel_key: SealingKey,
    pub receiving_channel_id: ChannelId,
    pub receiving_channel_key: OpeningKey,
}

pub struct HandshakeService<SR> {
    neighbors_table: Rc<RefCell<NeighborTable>>,
    my_public_key: PublicKey,

    secure_rng: Rc<SR>,
    rand_values_store: RandValuesStore,

    pre_handshake: HashMap<RandValue, (usize, PublicKey)>,
    handshaking: SessionTable<HandshakeState>,
}

impl<SR: SecureRandom> HandshakeService<SR> {
    pub fn new(
        neighbors_table: Rc<RefCell<NeighborTable>>,
        my_public_key: PublicKey,
        shared_rng: Rc<SR>
    ) -> HandshakeService<SR> {
        let rand_values_store = RandValuesStore::new(
            &*shared_rng,
            RAND_VALUE_TICKS,
            RAND_VALUE_STORE_CAPACITY
        );

        HandshakeService {
            neighbors_table,
            my_public_key,
            secure_rng: shared_rng,
            rand_values_store,
            handshaking: SessionTable::new(),
            pre_handshake: HashMap::new(),
        }
    }

    pub fn initiate_handshake(&mut self, remote_pk: PublicKey)
        -> Result<RequestNonce, HandshakeError>
    {
        for (_, public_key) in self.pre_handshake.values() {
            if *public_key == remote_pk {
                return Err(HandshakeError::AlreadyInProgress);
            }
        }

        let id = SessionId::new_initiator(remote_pk.clone());
        if self.handshaking.contains_session(&id) {
            return Err(HandshakeError::AlreadyInProgress);
        }

        // ===== Passed all check, initiate a new handshake =====

        let request_nonce = RequestNonce {
            request_rand_nonce: loop {
                let new_rand_value = RandValue::new(&*self.secure_rng);
                // We can not use an exist random value
                if !self.pre_handshake.contains_key(&new_rand_value) {
                    break new_rand_value;
                }
            },
        };

        self.pre_handshake.insert(
            request_nonce.request_rand_nonce.clone(),
            (REQUEST_NONCE_TIMEOUT, remote_pk)
        );

        Ok(request_nonce)
    }

    pub fn process_request_nonce(&mut self, request_nonce: RequestNonce)
        -> Result<ResponseNonce, HandshakeError>
    {
        let response_nonce = ResponseNonce {
            request_rand_nonce: request_nonce.request_rand_nonce,
            response_rand_nonce: RandValue::new(&*self.secure_rng),
            responder_rand_nonce: self.rand_values_store.last_rand_value(),

            signature: Signature::from(&[0x00; SIGNATURE_LEN]),
        };

        Ok(response_nonce)
    }

    fn verify_response_nonce(&self, response_nonce: &ResponseNonce)
        -> Result<(), HandshakeError>
    {
        // We sent this request before, and it still alive?
        let remote_public_key = self.pre_handshake
            .get(&response_nonce.request_rand_nonce)
            .map(|(_, pk)| pk.clone())
            .ok_or(HandshakeError::NoSuchSession)?;

        // The signature of the message is correct?
        if !verify_signature(
            &response_nonce.as_bytes(),
            &remote_public_key,
            &response_nonce.signature
        ) {
            return Err(HandshakeError::InvalidSignature);
        }

        Ok(())
    }

    pub fn process_response_nonce(&mut self, response_nonce: ResponseNonce)
        -> Result<ExchangeActive, HandshakeError>
    {
        self.verify_response_nonce(&response_nonce)?;

        let (_, remote_public_key) = self.pre_handshake
            .remove(&response_nonce.request_rand_nonce)
            .expect("no such pre-handshake session");

        let key_salt = Salt::new(&*self.secure_rng).map_err(HandshakeError::Crypto)?;
        let my_private_key = DhPrivateKey::new(&*self.secure_rng).map_err(HandshakeError::Crypto)?;
        let dh_public_key = my_private_key.compute_public_key().map_err(HandshakeError::Crypto)?;

        let exchange_active = ExchangeActive {
            key_salt,
            dh_public_key,

            responder_rand_nonce: response_nonce.responder_rand_nonce,
            initiator_rand_nonce: RandValue::new(&*self.secure_rng),
            initiator_public_key: self.my_public_key.clone(),
            responder_public_key: remote_public_key.clone(),
            signature: Signature::from(&[0x00; SIGNATURE_LEN]),
        };

        let id = SessionId::new_initiator(remote_public_key.clone());
        let state = HandshakeState::AfterInitiatorExchangeActive {
            my_private_key,
            sent_key_salt: exchange_active.key_salt.clone(),
            recv_rand_nonce: exchange_active.responder_rand_nonce.clone(),
            sent_rand_nonce: exchange_active.initiator_rand_nonce.clone(),
        };
        let last_hash = sha_512_256(&exchange_active.as_bytes());
        let new_session = HandshakeSession::new(id, state, last_hash);

        match self.handshaking.add_session(new_session) {
            None => Ok(exchange_active),
            Some(_) => Err(HandshakeError::SessionExists)
        }
    }

    fn verify_exchange_active(&self, exchange_active: &ExchangeActive)
        -> Result<(), HandshakeError>
    {
        let remote_public_key = exchange_active.initiator_public_key.clone();

        // The remote can initiate a new handshake?
        match self.neighbors_table.borrow_mut().get(&remote_public_key) {
            Some(neighbor) => {
                if neighbor.info.socket_addr.is_some() {
                    return Err(HandshakeError::NotAllowed);
                }
            },
            None => return Err(HandshakeError::NotAllowed),
        }

        // The signature of the message is correct?
        if !verify_signature(
            &exchange_active.as_bytes(),
            &remote_public_key,
            &exchange_active.signature
        ) {
            return Err(HandshakeError::InvalidSignature)
        }

        // The received nonce is contained in constant nonce list?
        if !self.rand_values_store.contains(&exchange_active.responder_rand_nonce) {
            return Err(HandshakeError::NotAllowed);
        }

        // If there already an in-flight associated with this neighbor?
        let id = SessionId::new_responder(remote_public_key);
        if self.handshaking.contains_session(&id) {
            return Err(HandshakeError::SessionExists);
        }

        Ok(())
    }

    pub fn process_exchange_active(&mut self, exchange_active: ExchangeActive)
        -> Result<ExchangePassive, HandshakeError>
    {
        self.verify_exchange_active(&exchange_active)?;

        let key_salt = Salt::new(&*self.secure_rng).map_err(HandshakeError::Crypto)?;
        let my_private_key = DhPrivateKey::new(&*self.secure_rng).map_err(HandshakeError::Crypto)?;
        let dh_public_key = my_private_key.compute_public_key().map_err(HandshakeError::Crypto)?;

        let exchange_passive = ExchangePassive {
            prev_hash: sha_512_256(&exchange_active.as_bytes()),
            dh_public_key,
            key_salt,

            signature: Signature::from(&[0x00; SIGNATURE_LEN]),
        };

        let id = SessionId::new_responder(exchange_active.initiator_public_key);
        let state = HandshakeState::AfterResponderExchangePassive {
            sent_key_salt: exchange_passive.key_salt.clone(),
            recv_key_salt: exchange_active.key_salt,
            sent_rand_nonce: exchange_active.responder_rand_nonce,
            recv_rand_nonce: exchange_active.initiator_rand_nonce,
            remote_dh_public_key: exchange_active.dh_public_key,
            my_private_key,
        };
        let last_hash = sha_512_256(&exchange_passive.as_bytes());
        let new_session = HandshakeSession::new(id, state, last_hash);

        match self.handshaking.add_session(new_session) {
            None => Ok(exchange_passive),
            Some(_) => Err(HandshakeError::SessionExists),
        }
    }

    fn verify_exchange_passive(&self, exchange_passive: &ExchangePassive)
        -> Result<(), HandshakeError>
    {
        let opt_session = self.handshaking.get_session(&exchange_passive.prev_hash);

        let remote_public_key = match opt_session {
            None => return Err(HandshakeError::NoSuchSession),
            Some(session) => {
                // Verify the current handshake state
                if session.is_after_initiator_exchange_active() {
                    session.remote_public_key().clone()
                } else {
                    return Err(HandshakeError::InvalidTransfer)
                }
            }
        };

        // The signature of the message is correct?
        if !verify_signature(
            &exchange_passive.as_bytes(),
            &remote_public_key,
            &exchange_passive.signature
        ) {
            return Err(HandshakeError::InvalidSignature);
        }

        Ok(())
    }

    pub fn process_exchange_passive(&mut self, exchange_passive: ExchangePassive)
        -> Result<(HandshakeResult, ChannelReady), HandshakeError>
    {
        self.verify_exchange_passive(&exchange_passive)?;

        let channel_ready = ChannelReady {
            prev_hash: sha_512_256(&exchange_passive.as_bytes()),
            signature: Signature::from(&[0x00; SIGNATURE_LEN]),
        };

        let old_session = self.handshaking
            .remove_by_last_hash(&exchange_passive.prev_hash)
            .expect("invalid message passed the verification!");

        let handshake_result = old_session.initiator_finish(
            exchange_passive.key_salt,
            exchange_passive.dh_public_key
        )?;

        Ok((handshake_result, channel_ready))
    }

    fn verify_channel_ready(&self, channel_ready: &ChannelReady)
        -> Result<(), HandshakeError>
    {
        let opt_session = self.handshaking.get_session(&channel_ready.prev_hash);

        let remote_public_key = match opt_session {
            None => return Err(HandshakeError::NoSuchSession),
            Some(session) => {
                // Verify the current handshake state
                if session.is_after_responder_exchange_passive() {
                    session.remote_public_key().clone()
                } else {
                    return Err(HandshakeError::InvalidTransfer)
                }
            }
        };

        if !verify_signature(
            &channel_ready.as_bytes(),
            &remote_public_key,
            &channel_ready.signature
        ) {
            return Err(HandshakeError::InvalidSignature);
        }

        Ok(())
    }

    pub fn process_channel_ready(&mut self, channel_ready: ChannelReady)
        -> Result<HandshakeResult, HandshakeError>
    {
        self.verify_channel_ready(&channel_ready)?;

        let old_session = self.handshaking
            .remove_by_last_hash(&channel_ready.prev_hash)
            .expect("invalid message passed the verification!");

        old_session.responder_finish()
    }

    pub fn time_tick(&mut self) {
        self.handshaking.time_tick();
        self.rand_values_store.time_tick(&*self.secure_rng);

        self.pre_handshake.retain(|_k, (timeout_counter, _)| {
            if *timeout_counter <= 1 {
                false
            } else {
                *timeout_counter -= 1;
                true
            }
        })
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

    const TICKS: usize = 100;

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

        let mut hs_state_machine_a = HandshakeService::new(
            neighbors_a,
            public_key_a.clone(),
            Rc::clone(&shared_secure_rng),
        );

        let mut hs_state_machine_b = HandshakeService::new(
            neighbors_b,
            public_key_b.clone(),
            Rc::clone(&shared_secure_rng),
        );

        // A -> B: RequestNonce
        let request_nonce_to_b = hs_state_machine_a.initiate_handshake(
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
        let mut exchange_active_to_b = hs_state_machine_a.process_response_nonce(
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

        assert_eq!(new_channel_info_a.sending_channel_id, new_channel_info_b.receiving_channel_id);
        assert_eq!(new_channel_info_b.sending_channel_id, new_channel_info_a.receiving_channel_id);
    }
}
