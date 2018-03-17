use std::rc::Rc;
use std::cell::RefCell;

use ring::rand::SecureRandom;
use bytes::Bytes;

use crypto::dh::{DhPrivateKey, Salt};
use crypto::identity::{PublicKey, verify_signature, SIGNATURE_LEN, Signature};
use crypto::hash::{HashResult, sha_512_256};
use crypto::rand_values::RandValue;
use proto::channeler::{InitChannel, ExchangeActive, ExchangePassive, ChannelReady};
use channeler::types::NeighborsTable;

use super::Result;
use super::types::*;

/// The state of a handshake session.
pub enum State {
    InitChannel {
        init_channel: InitChannel,
    },

    Passive {
        init_channel: InitChannel,
        my_private_key: DhPrivateKey,
        exchange_passive: ExchangePassive,
    },

    Active {
        init_channel: InitChannel,
        exchange_passive: ExchangePassive,
        my_private_key: DhPrivateKey,
        exchange_active: ExchangeActive,
    },
}

impl State {
    pub fn new(init_channel: InitChannel) -> State {
        State::InitChannel { init_channel }
    }

    pub fn move_passive(
        self,
        my_private_key: DhPrivateKey,
        exchange_passive: ExchangePassive,
    ) -> Result<State> {
        match self {
            State::InitChannel { init_channel } => {
                Ok(State::Passive {
                    init_channel,
                    my_private_key,
                    exchange_passive,
                })
            }
            _ => Err(HandshakeError::InvalidTransfer)
        }
    }

    pub fn move_active(self, exchange_active: ExchangeActive) -> Result<State> {
        match self {
            State::Passive { init_channel, my_private_key, exchange_passive } => {
                Ok(State::Active {
                    init_channel,
                    my_private_key,
                    exchange_passive,
                    exchange_active,
                })
            }
            _ => Err(HandshakeError::InvalidTransfer)
        }
    }
}

pub struct HandshakeStateMachine<SR> {
    neighbors: Rc<RefCell<NeighborsTable>>,
    secure_rng: Rc<SR>,
    sessions_map: HandshakeSessionMap,
    self_public_key: PublicKey,
}

impl<SR: SecureRandom> HandshakeStateMachine<SR> {
    pub fn new(
        neighbors: Rc<RefCell<NeighborsTable>>,
        secure_rng: Rc<SR>,
        public_key: PublicKey,
    ) -> HandshakeStateMachine<SR> {
        HandshakeStateMachine {
            neighbors,
            secure_rng,
            self_public_key: public_key,
            sessions_map: HandshakeSessionMap::new(),
        }
    }

    pub fn new_handshake(&mut self, neighbor_public_key: PublicKey) -> Result<InitChannel> {
        let id = HandshakeId::new(HandshakeRole::Initiator, neighbor_public_key);

        if self.sessions_map.contains_id(&id) {
            return Err(HandshakeError::AlreadyExist);
        }

        let rand_nonce = RandValue::new(&*self.secure_rng);
        let init_channel = InitChannel {
            rand_nonce,
            public_key: self.self_public_key.clone(),
        };

        let last_hash = sha_512_256(&init_channel.as_bytes());

        let state = State::new(init_channel.clone());

        let session = HandshakeSession::new(id, state, 100);

        match self.sessions_map.insert(last_hash, session) {
            None => Ok(init_channel),
            Some(_) => Err(HandshakeError::AlreadyExist),
        }
    }

    pub fn process_init_channel(&mut self, init_channel: InitChannel) -> Result<ExchangePassive> {
        let id = HandshakeId::new(HandshakeRole::Responder, init_channel.public_key.clone());

        if self.sessions_map.contains_id(&id) {
            return Err(HandshakeError::AlreadyExist);
        } else {
            match self.neighbors.borrow().get(&init_channel.public_key) {
                Some(neighbor) if neighbor.info.socket_addr.is_none() => (),
                _ => return Err(HandshakeError::NotAllowed),
            }
        }

        let prev_hash = sha_512_256(&init_channel.as_bytes());
        let rand_nonce = RandValue::new(&*self.secure_rng);
        let dh_private_key = DhPrivateKey::new(&*self.secure_rng)
            .map_err(|e| HandshakeError::CryptoError(e))?;
        let dh_public_key = dh_private_key.compute_public_key()
            .map_err(|e| HandshakeError::CryptoError(e))?;
        let key_salt = Salt::new(&*self.secure_rng)
            .map_err(|e| HandshakeError::CryptoError(e))?;

        let exchange_passive = ExchangePassive {
            prev_hash,
            rand_nonce,
            public_key: self.self_public_key.clone(),
            dh_public_key,
            key_salt,
            signature: Signature::from(&[0x00u8; SIGNATURE_LEN]),
        };

        let last_hash = sha_512_256(&exchange_passive.as_bytes());

        let state = State::new(init_channel);
        let state = state.move_passive(dh_private_key, exchange_passive.clone())?;

        let session = HandshakeSession::new(id, state, 100);

        match self.sessions_map.insert(last_hash, session) {
            None => Ok(exchange_passive),
            Some(_) => Err(HandshakeError::AlreadyExist),
        }
    }

    pub fn process_channel_ready(&mut self, channel_ready: ChannelReady) -> Result<NewChannelInfo> {
        let session = self.sessions_map.take_by_hash(&channel_ready.prev_hash)
            .ok_or(HandshakeError::NoSuchSession)?;

        let remote_public_key = session.remote_public_key();

        if !verify_signature(&channel_ready.prev_hash, remote_public_key, &channel_ready.signature) {
            return Err(HandshakeError::InvalidSignature);
        }

        session.finish()
    }

    pub fn process_exchange_active(&mut self, exchange_active: ExchangeActive) -> Result<(NewChannelInfo, ChannelReady)> {
        let session = self.sessions_map.take_by_hash(&exchange_active.prev_hash)
            .ok_or(HandshakeError::NoSuchSession)?;

        let remote_public_key = session.remote_public_key();

        if !verify_signature(&exchange_active.as_bytes(), remote_public_key, &exchange_active.signature) {
            return Err(HandshakeError::InvalidSignature);
        }

        let channel_ready = ChannelReady {
            prev_hash: sha_512_256(&exchange_active.as_bytes()),
            signature: Signature::from(&[0x00u8; SIGNATURE_LEN]),
        };

//        let last_hash = sha_512_256(&channel_ready.prev_hash);
        let id = session.id;
        let state = session.state.move_active(exchange_active)?;

        // Rebuild a session, also reset the timeout counter
        // FIXME: Change the design
        let session = HandshakeSession::new(id, state, 100);

        let new_channel_info = session.finish()?;

        Ok((new_channel_info, channel_ready))
    }

    pub fn process_exchange_passive(&mut self, exchange_passive: ExchangePassive) -> Result<ExchangeActive> {
        let session = self.sessions_map.take_by_hash(&exchange_passive.prev_hash)
            .ok_or(HandshakeError::NoSuchSession)?;

        let remote_public_key = session.remote_public_key();

        if !verify_signature(&exchange_passive.as_bytes(), remote_public_key, &exchange_passive.signature) {
            return Err(HandshakeError::InvalidSignature);
        }

        let prev_hash = sha_512_256(&exchange_passive.as_bytes());
        let dh_private_key = DhPrivateKey::new(&*self.secure_rng)
            .map_err(|e| HandshakeError::CryptoError(e))?;
        let dh_public_key = dh_private_key.compute_public_key()
            .map_err(|e| HandshakeError::CryptoError(e))?;
        let key_salt = Salt::new(&*self.secure_rng)
            .map_err(|e| HandshakeError::CryptoError(e))?;

        let exchange_active = ExchangeActive {
            prev_hash,
            dh_public_key,
            key_salt,
            signature: Signature::from(&[0x00u8; SIGNATURE_LEN]),
        };

        let last_hash = sha_512_256(&exchange_active.as_bytes());
        let id = session.id;
        let state = session.state.move_passive(dh_private_key, exchange_passive)?;
        let state = state.move_active(exchange_active.clone())?;

        // Rebuild a session, also reset the timeout counter
        let session = HandshakeSession::new(id, state, 100);

        match self.sessions_map.insert(last_hash, session) {
            None => Ok(exchange_active),
            Some(_) => Err(HandshakeError::AlreadyExist),
        }
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

        let mut handshake_state_machine_a = HandshakeStateMachine::new(
            neighbors_a,
            Rc::clone(&shared_secure_rng),
            public_key_a.clone(),
        );

        let mut handshake_state_machine_b = HandshakeStateMachine::new(
            neighbors_b,
            Rc::clone(&shared_secure_rng),
            public_key_b.clone(),
        );

        // A -> B
        let init_channel_to_b =
            handshake_state_machine_a.new_handshake(public_key_b.clone()).unwrap();
        // B -> A
        let mut exchange_passive_to_a =
            handshake_state_machine_b.process_init_channel(init_channel_to_b).unwrap();
        exchange_passive_to_a.signature =
            identity_b.sign_message(&exchange_passive_to_a.as_bytes());
        // A -> B
        let mut exchange_active_to_b =
            handshake_state_machine_a.process_exchange_passive(exchange_passive_to_a).unwrap();
        exchange_active_to_b.signature =
            identity_a.sign_message(&exchange_active_to_b.as_bytes());
        // B -> A
        let (new_channel_info_b, mut channel_ready_to_a) =
            handshake_state_machine_b.process_exchange_active(exchange_active_to_b).unwrap();
        channel_ready_to_a.signature = identity_b.sign_message(&channel_ready_to_a.prev_hash);

        let new_channel_info_a =
            handshake_state_machine_a.process_channel_ready(channel_ready_to_a).unwrap();

        assert_eq!(new_channel_info_a.sender_id, new_channel_info_b.receiver_id);
        assert_eq!(new_channel_info_b.receiver_id, new_channel_info_a.sender_id);
    }
}