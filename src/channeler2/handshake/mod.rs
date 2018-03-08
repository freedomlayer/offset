use std::cell::RefCell;
use std::rc::Rc;

use futures::prelude::*;
use futures::sync::{mpsc, oneshot};
use ring::aead::{OpeningKey, SealingKey};
use ring::rand::SecureRandom;
use tokio_core::reactor::Handle;

use channeler2::{ChannelId, NeighborInfo, NeighborsTable, CHANNEL_ID_LEN};
use crypto::dh::{DhPrivateKey, Salt};
use crypto::hash::{HashResult, sha_512_256};
use crypto::identity::{PublicKey, verify_signature};
use crypto::rand_values::RandValue;
use proto::channeler_udp::*;
use security_module::client::SecurityModuleClient;

mod types;
use self::types::*;
use super::channel::NewChannelInfo;

pub enum ToHandshakeManager {
    TimerTick,

    NewHandshake {
        neighbor_public_key: PublicKey,
        response_sender:     oneshot::Sender<InitChannel>,
    },

    InitChannel {
        init_channel:    InitChannel,
        response_sender: oneshot::Sender<ExchangePassive>,
    },

    ChannelReady {
        channel_ready:   ChannelReady,
        response_sender: oneshot::Sender<NewChannelInfo>,
    },

    ExchangeActive {
        exchange_active: ExchangeActive,
        response_sender: oneshot::Sender<(NewChannelInfo, ChannelReady)>,
    },

    ExchangePassive {
        exchange_passive: ExchangePassive,
        response_sender:  oneshot::Sender<ExchangeActive>,
    },
}

/// The handshake manager, which manage all pending 4-way handshakes
pub struct HandshakeManager<SR: SecureRandom> {
    /// The reactor handle for spawning new task
    handle: Handle,

    /// The in progress handshakes table
    ///
    /// This table needs to share with the tasks
    handshakes: Rc<RefCell<HandshakeTable>>,

    /// Secure random number generator
    secure_rng: Rc<SR>,

    /// Security module client
    ///
    /// When creating a new task, this will be cloned
    sm_client: SecurityModuleClient,

    /// The neighbors table of the `Channeler`
    ///
    /// This table needs to share with tasks
    neighbors: Rc<RefCell<NeighborsTable>>,

    /// The receiver used to receive the request from `Channeler`
    request_receiver: mpsc::Receiver<ToHandshakeManager>,

    /// The handshake timeout ticks
    handshake_timeout_ticks: usize,
}

#[derive(Debug)]
pub enum HandshakeManagerError {
    SecurityModuleClientError,

    /// Error may occur when polling the next request
    PollRequestError,

    /// Error may occur when sending back the response
    SendResponseError,

    /// Not allowed
    NotAllowed,

    InvalidHandshake,

    VerificationFailed,

    /// Other error may occurred in processing handshake.
    ///
    /// THIS ERROR ALWAYS INDICATE A BUG!
    Other,
}

impl<SR: SecureRandom + 'static> HandshakeManager<SR> {
    pub fn new(
        handle: Handle,
        neighbors: Rc<RefCell<NeighborsTable>>,
        sm_client: SecurityModuleClient,
        secure_rng: Rc<SR>,
        request_receiver: mpsc::Receiver<ToHandshakeManager>,
        handshake_timeout_ticks: usize,
    ) -> HandshakeManager<SR> {
        HandshakeManager {
            handle,
            neighbors,
            sm_client,
            secure_rng,
            request_receiver,
            handshake_timeout_ticks,
            handshakes: Rc::new(RefCell::new(HandshakeTable::new())),
        }
    }

    fn process_timer_tick(&mut self) {
        info!("Removed {} handshakes", self.handshakes.borrow_mut().tick());
    }

    fn new_handshake(
        &self,
        neighbor_public_key: PublicKey,
        response_sender: oneshot::Sender<InitChannel>,
    ) {
        let sm_client = self.sm_client.clone();
        let handshakes = Rc::clone(&self.handshakes);
        let secure_rng = Rc::clone(&self.secure_rng);
        let timeout_ticks = self.handshake_timeout_ticks;

        let validation = move || {
            let id = HandshakeId::new(HandshakeRole::Initiator, neighbor_public_key);

            if handshakes.borrow().contains_id(&id) {
                Err(HandshakeManagerError::NotAllowed)
            } else {
                Ok((id, handshakes))
            }
        };

        let new_handshake_task = validation()
            .into_future()
            .and_then(move |(id, handshakes)| {
                let rand_nonce = RandValue::new(&*secure_rng);

                sm_client
                    .request_public_key()
                    .map_err(|_| HandshakeManagerError::SecurityModuleClientError)
                    .and_then(move |public_key| {
                        let init_channel = InitChannel {
                            rand_nonce,
                            public_key,
                        };
                        Ok((id, handshakes, init_channel))
                    })
            })
            .and_then(move |(id, handshakes, init_channel)| {
                response_sender
                    .send(init_channel.clone())
                    .map_err(|_| HandshakeManagerError::SendResponseError)
                    .and_then(move |_| Ok((id, handshakes, init_channel)))
            })
            .and_then(move |(id, handshakes, init_channel)| {
                let last_message_hash = hash_of_init_channel(&init_channel);

                let handshake = Handshake {
                    id,
                    timeout_ticks,
                    init_channel: Some(init_channel),
                    my_private_key: None,
                    exchange_active: None,
                    exchange_passive: None,
                };

                handshakes.borrow_mut().insert(last_message_hash, handshake);

                Ok(())
            });

        self.handle.spawn(new_handshake_task.map_err(|e| {
            info!("new handshake task encountered an error: {:?}", e);
        }));
    }

    #[cfg_attr(rustfmt, rustfmt_skip)]
    fn process_init_channel(
        &self,
        init_channel: InitChannel,
        response_sender: oneshot::Sender<ExchangePassive>,
    ) {
        let handshakes = Rc::clone(&self.handshakes);
        let neighbors = Rc::clone(&self.neighbors);
        let sm_client = self.sm_client.clone();
        let secure_rng = Rc::clone(&self.secure_rng);
        let timeout_ticks = self.handshake_timeout_ticks;

        let validation = move || {
            let id = HandshakeId::new(HandshakeRole::Responder, init_channel.public_key.clone());

            if handshakes.borrow().contains_id(&id) {
                Err(HandshakeManagerError::NotAllowed)
            } else {
                match neighbors.borrow().get(&init_channel.public_key) {
                    Some(neighbor) if neighbor.socket_addr.is_none() => {
                        Ok((handshakes, id, init_channel))
                    }
                    _ => Err(HandshakeManagerError::NotAllowed),
                }
            }
        };

        let process_init_channel_task = validation()
            .into_future()
            .and_then(move |(handshakes, id, init_channel)| {
                sm_client
                    .request_public_key()
                    .map_err(|_| HandshakeManagerError::SecurityModuleClientError)
                    .and_then(move |public_key| {
                        Ok((handshakes, id, init_channel, sm_client, public_key))
                    })
            })
            .and_then(move |(handshakes, id, init_channel, sm_client, public_key)| {
                let prev_hash = hash_of_init_channel(&init_channel);
                let rand_nonce = RandValue::new(&*secure_rng);
                let dh_private_key = DhPrivateKey::new(&*secure_rng);
                let dh_public_key = dh_private_key.compute_public_key();
                let key_salt = Salt::new(&*secure_rng);

                let handshake = Handshake {
                    id,
                    timeout_ticks,
                    init_channel: Some(init_channel),
                    my_private_key: Some(dh_private_key),
                    exchange_active: None,
                    exchange_passive: None,
                };

                let mut msg_to_sign = Vec::new();
                msg_to_sign.extend_from_slice(prev_hash.as_ref());
                msg_to_sign.extend_from_slice(rand_nonce.as_ref());
                msg_to_sign.extend_from_slice(public_key.as_ref());
                msg_to_sign.extend_from_slice(dh_public_key.as_ref());
                msg_to_sign.extend_from_slice(key_salt.as_ref());

                sm_client
                    .request_signature(msg_to_sign)
                    .map_err(|_| HandshakeManagerError::SecurityModuleClientError)
                    .and_then(move |signature| {
                        let exchange_passive = ExchangePassive {
                            prev_hash,
                            rand_nonce,
                            public_key,
                            dh_public_key,
                            key_salt,
                            signature,
                        };

                        Ok((handshakes, handshake, exchange_passive))
                    })
            })
            .and_then(move |(handshakes, handshake, exchange_passive)| {
                response_sender
                    .send(exchange_passive.clone())
                    .map_err(|_| HandshakeManagerError::SendResponseError)
                    .and_then(move |_| Ok((handshakes, handshake, exchange_passive)))
            })
            .and_then(move |(handshakes, mut handshake, exchange_passive)| {
                let last_message_hash = hash_of_exchange_passive(&exchange_passive);
                handshake.exchange_passive = Some(exchange_passive);
                handshakes.borrow_mut().insert(last_message_hash, handshake);
                Ok(())
            });

        self.handle.spawn(process_init_channel_task.map_err(|e| {
            info!("process init channel task encountered an error: {:?}", e);
        }));
    }

    fn process_channel_ready(
        &mut self,
        channel_ready: ChannelReady,
        response_sender: oneshot::Sender<NewChannelInfo>,
    ) {
        let take_handshake = self.take_handshake(&channel_ready.prev_hash);

        let validation = move || {
            let handshake = take_handshake.ok_or(HandshakeManagerError::NotAllowed)?;
            let remote_public_key = handshake.remote_public_key();

            let mut msg_to_verify = Vec::new();
            msg_to_verify.extend_from_slice(channel_ready.prev_hash.as_ref());
            if verify_signature(&msg_to_verify, remote_public_key, &channel_ready.signature) {
                Ok(handshake)
            } else {
                Err(HandshakeManagerError::NotAllowed)
            }
        };

        let process_channel_ready_task = validation().into_future().and_then(move |handshake| {
            handshake.finish().and_then(move |new_channel_info| {
                response_sender
                    .send(new_channel_info)
                    .map_err(|_| HandshakeManagerError::SendResponseError)
            })
        });

        self.handle.spawn(process_channel_ready_task.map_err(|e| {
            info!("process channel ready task encountered an error: {:?}", e);
        }));
    }

    fn process_exchange_active(
        &mut self,
        exchange_active: ExchangeActive,
        response_sender: oneshot::Sender<(NewChannelInfo, ChannelReady)>,
    ) {
        let sm_client = self.sm_client.clone();
        let take_handshake = self.take_handshake(&exchange_active.prev_hash);

        let validation = move || {
            let handshake = take_handshake.ok_or(HandshakeManagerError::NotAllowed)?;
            let remote_public_key = handshake.remote_public_key();

            let mut msg_to_verify = Vec::new();
            msg_to_verify.extend_from_slice(exchange_active.prev_hash.as_ref());
            msg_to_verify.extend_from_slice(exchange_active.dh_public_key.as_ref());
            msg_to_verify.extend_from_slice(exchange_active.key_salt.as_ref());
            if verify_signature(
                &msg_to_verify,
                remote_public_key,
                &exchange_active.signature,
            ) {
                Ok((handshake, exchange_active))
            } else {
                Err(HandshakeManagerError::VerificationFailed)
            }
        };

        let process_exchange_active_task = validation()
            .into_future()
            .and_then(move |(mut handshake, exchange_active)| {
                let prev_hash = hash_of_exchange_active(&exchange_active);
                handshake.exchange_active = Some(exchange_active);

                let mut msg_to_sign = Vec::new();
                msg_to_sign.extend_from_slice(prev_hash.as_ref());

                sm_client
                    .request_signature(msg_to_sign)
                    .map_err(|_| HandshakeManagerError::SecurityModuleClientError)
                    .and_then(move |signature| {
                        handshake.finish().and_then(move |new_channel_info| {
                            let channel_ready = ChannelReady {
                                prev_hash,
                                signature,
                            };

                            Ok((new_channel_info, channel_ready))
                        })
                    })
            })
            .and_then(move |(new_channel_info, channel_ready)| {
                response_sender
                    .send((new_channel_info, channel_ready))
                    .map_err(|_| HandshakeManagerError::SendResponseError)
            });

        self.handle.spawn(process_exchange_active_task.map_err(|e| {
            info!("process exchange active task encountered an error: {:?}", e);
        }));
    }

    fn process_exchange_passive(
        &mut self,
        exchange_passive: ExchangePassive,
        response_sender: oneshot::Sender<ExchangeActive>,
    ) {
        let sm_client = self.sm_client.clone();
        let handshakes = Rc::clone(&self.handshakes);
        let secure_rng = Rc::clone(&self.secure_rng);
        let take_handshake = self.take_handshake(&exchange_passive.prev_hash);

        let validation = move || {
            let handshake = take_handshake.ok_or(HandshakeManagerError::NotAllowed)?;
            let remote_public_key = handshake.remote_public_key();

            let mut msg_to_verify = Vec::new();
            msg_to_verify.extend_from_slice(exchange_passive.prev_hash.as_ref());
            msg_to_verify.extend_from_slice(exchange_passive.rand_nonce.as_ref());
            msg_to_verify.extend_from_slice(exchange_passive.public_key.as_ref());
            msg_to_verify.extend_from_slice(exchange_passive.dh_public_key.as_ref());
            msg_to_verify.extend_from_slice(exchange_passive.key_salt.as_ref());
            if verify_signature(
                &msg_to_verify,
                remote_public_key,
                &exchange_passive.signature,
            ) {
                Ok((handshake, exchange_passive))
            } else {
                Err(HandshakeManagerError::VerificationFailed)
            }
        };

        let process_exchange_passive_task = validation()
            .into_future()
            .and_then(move |(mut handshake, exchange_passive)| {
                let prev_hash = hash_of_exchange_passive(&exchange_passive);
                let dh_private_key = DhPrivateKey::new(&*secure_rng);
                let dh_public_key = dh_private_key.compute_public_key();
                let key_salt = Salt::new(&*secure_rng);

                handshake.exchange_passive = Some(exchange_passive);
                handshake.my_private_key = Some(dh_private_key);

                let mut msg_to_sign = Vec::new();
                msg_to_sign.extend_from_slice(prev_hash.as_ref());
                msg_to_sign.extend_from_slice(dh_public_key.as_ref());
                msg_to_sign.extend_from_slice(key_salt.as_ref());

                sm_client
                    .request_signature(msg_to_sign)
                    .map_err(|_| HandshakeManagerError::SecurityModuleClientError)
                    .and_then(move |signature| {
                        let exchange_active = ExchangeActive {
                            prev_hash,
                            dh_public_key,
                            key_salt,
                            signature,
                        };

                        Ok((handshake, exchange_active))
                    })
            })
            .and_then(move |(handshake, exchange_active)| {
                response_sender
                    .send(exchange_active.clone())
                    .map_err(|_| HandshakeManagerError::SendResponseError)
                    .and_then(move |_| Ok((handshake, exchange_active)))
            })
            .and_then(move |(mut handshake, exchange_active)| {
                let last_message_hash = hash_of_exchange_active(&exchange_active);
                handshake.exchange_active = Some(exchange_active);
                handshakes.borrow_mut().insert(last_message_hash, handshake);

                Ok(())
            });

        self.handle
            .spawn(process_exchange_passive_task.map_err(|e| {
                info!("process init channel task encountered an error: {:?}", e);
            }));
    }

    fn take_handshake(&mut self, hash: &HashResult) -> Option<Handshake> {
        self.handshakes.borrow_mut().remove(hash)
    }
}

impl<SR: SecureRandom + 'static> Future for HandshakeManager<SR> {
    type Item = ();
    type Error = HandshakeManagerError;

    fn poll(&mut self) -> Poll<(), HandshakeManagerError> {
        // TODO: We may have to control how many tasks in-flight, consider expose runtime metrics?
        loop {
            let poll_result = self.request_receiver
                .poll()
                .map_err(|_| HandshakeManagerError::PollRequestError);

            match try_ready!(poll_result) {
                None => return Ok(Async::Ready(())),
                Some(request) => match request {
                    ToHandshakeManager::TimerTick => self.process_timer_tick(),
                    ToHandshakeManager::NewHandshake {
                        neighbor_public_key,
                        response_sender,
                    } => {
                        self.new_handshake(neighbor_public_key, response_sender);
                    }
                    ToHandshakeManager::InitChannel {
                        init_channel,
                        response_sender,
                    } => {
                        self.process_init_channel(init_channel, response_sender);
                    }
                    ToHandshakeManager::ChannelReady {
                        channel_ready,
                        response_sender,
                    } => {
                        self.process_channel_ready(channel_ready, response_sender);
                    }
                    ToHandshakeManager::ExchangeActive {
                        exchange_active,
                        response_sender,
                    } => {
                        self.process_exchange_active(exchange_active, response_sender);
                    }
                    ToHandshakeManager::ExchangePassive {
                        exchange_passive,
                        response_sender,
                    } => {
                        self.process_exchange_passive(exchange_passive, response_sender);
                    }
                },
            }
        }
    }
}

fn hash_of_init_channel(init_channel: &InitChannel) -> HashResult {
    let mut data = Vec::new();
    data.extend_from_slice(init_channel.rand_nonce.as_ref());
    data.extend_from_slice(init_channel.public_key.as_ref());
    sha_512_256(&data)
}

fn hash_of_exchange_active(exchange_active: &ExchangeActive) -> HashResult {
    let mut data = Vec::new();
    data.extend_from_slice(exchange_active.prev_hash.as_ref());
    data.extend_from_slice(exchange_active.dh_public_key.as_ref());
    data.extend_from_slice(exchange_active.key_salt.as_ref());
    data.extend_from_slice(exchange_active.signature.as_ref());
    sha_512_256(&data)
}

fn hash_of_exchange_passive(exchange_passive: &ExchangePassive) -> HashResult {
    let mut data = Vec::new();
    data.extend_from_slice(exchange_passive.prev_hash.as_ref());
    data.extend_from_slice(exchange_passive.rand_nonce.as_ref());
    data.extend_from_slice(exchange_passive.public_key.as_ref());
    data.extend_from_slice(exchange_passive.dh_public_key.as_ref());
    data.extend_from_slice(exchange_passive.key_salt.as_ref());
    data.extend_from_slice(exchange_passive.signature.as_ref());
    sha_512_256(&data)
}

#[cfg(test)]
mod tests {
    use super::*;
    use channeler2::{NeighborInfo, NeighborsTable};
    use crypto::identity::{Identity, PublicKey};
    use crypto::identity::SoftwareEd25519Identity;
    use futures::sync::mpsc::channel;
    use ring::rand::SystemRandom;
    use ring::signature::Ed25519KeyPair;
    use ring::test::rand::FixedByteRandom;
    use security_module::client::SecurityModuleClient;
    use security_module::create_security_module;
    use std::collections::HashMap;

    use std::time::Duration;
    use tokio_core::reactor::Timeout;

    const TICKS: usize = 100;

    use tokio_core::reactor::{Core, Handle};

    fn gen_neighbor(byte: u8) -> NeighborInfo {
        let key_bytes = [byte; 32];
        let public_key = PublicKey::from_bytes(&key_bytes).unwrap();

        NeighborInfo {
            public_key,
            socket_addr: None,
            retry_ticks: 100,
        }
    }

    #[test]
    fn basic_handshake() {
        let (public_key_a, sm_sender_a, sm_a) = {
            let fixed_rand = FixedByteRandom { byte: 0x00 };
            let pkcs8 = Ed25519KeyPair::generate_pkcs8(&fixed_rand).unwrap();
            let identity = SoftwareEd25519Identity::from_pkcs8(&pkcs8).unwrap();
            let public_key = identity.get_public_key();
            let (sm_sender, sm) = create_security_module(identity);

            (public_key, sm_sender, sm)
        };

        let (public_key_b, sm_sender_b, sm_b) = {
            let fixed_rand = FixedByteRandom { byte: 0x01 };
            let pkcs8 = Ed25519KeyPair::generate_pkcs8(&fixed_rand).unwrap();
            let identity = SoftwareEd25519Identity::from_pkcs8(&pkcs8).unwrap();
            let public_key = identity.get_public_key();
            let (sm_sender, sm) = create_security_module(identity);

            (public_key, sm_sender, sm)
        };

        let info_a = NeighborInfo {
            public_key:  public_key_a,
            socket_addr: None,
            retry_ticks: TICKS,
        };
        let info_b = NeighborInfo {
            public_key:  public_key_b,
            socket_addr: Some("127.0.0.1:10001".parse().unwrap()),
            retry_ticks: TICKS,
        };

        let (neighbors_a, neighbors_b) = {
            let mut neighbors_a = HashMap::new();
            neighbors_a.insert(info_b.public_key.clone(), info_b.clone());

            let mut neighbors_b = HashMap::new();
            neighbors_b.insert(info_a.public_key.clone(), info_a.clone());

            (
                Rc::new(RefCell::new(neighbors_a)),
                Rc::new(RefCell::new(neighbors_b)),
            )
        };

        let shared_secure_rng = Rc::new(SystemRandom::new());

        let mut core = Core::new().unwrap();
        let handle = core.handle();

        let (tx_a, rx_a) = channel(0);

        let handshake_manager_a = HandshakeManager::new(
            handle.clone(),
            neighbors_a,
            SecurityModuleClient::new(sm_sender_a),
            Rc::clone(&shared_secure_rng),
            rx_a,
            TICKS,
        );

        let (tx_b, rx_b) = channel(0);

        let handshake_manager_b = HandshakeManager::new(
            handle.clone(),
            neighbors_b,
            SecurityModuleClient::new(sm_sender_b),
            Rc::clone(&shared_secure_rng),
            rx_b,
            TICKS,
        );

        handle.spawn(sm_a.then(|_| Ok(())));
        handle.spawn(handshake_manager_a.map_err(|e| panic!("a encountered an error {:?}", e)));

        handle.spawn(sm_b.then(|_| Ok(())));
        handle.spawn(handshake_manager_b.map_err(|e| panic!("b encountered an error {:?}", e)));

        let (response_sender, response_receiver) = oneshot::channel();

        let handshake_task = tx_a.send(ToHandshakeManager::NewHandshake {
            neighbor_public_key: info_b.public_key.clone(),
            response_sender,
        }).map_err(|_| panic!("failed to send request to A[1]"))
            .and_then(move |tx_a| {
                response_receiver
                    .map_err(|_| panic!("failed to receive response from A[1]"))
                    .and_then(move |init_channel| {
                        let (response_sender, response_receiver) = oneshot::channel();

                        tx_b.send(ToHandshakeManager::InitChannel {
                            init_channel,
                            response_sender,
                        }).map_err(|_| panic!("failed to send request to B[1]"))
                            .and_then(move |tx_b| Ok((tx_a, tx_b, response_receiver)))
                    })
            })
            .and_then(move |(tx_a, tx_b, response_receiver)| {
                response_receiver
                    .map_err(|_| panic!("failed to receive response from B[1]"))
                    .and_then(move |exchange_passive| {
                        let (response_sender, response_receiver) = oneshot::channel();

                        tx_a.send(ToHandshakeManager::ExchangePassive {
                            exchange_passive,
                            response_sender,
                        }).map_err(|_| panic!("failed to send request to A[2]"))
                            .and_then(move |tx_a| Ok((tx_a, tx_b, response_receiver)))
                    })
            })
            .and_then(move |(tx_a, tx_b, response_receiver)| {
                response_receiver
                    .map_err(|_| panic!("failed to receive response from A[2]"))
                    .and_then(move |exchange_active| {
                        let (response_sender, response_receiver) = oneshot::channel();

                        tx_b.send(ToHandshakeManager::ExchangeActive {
                            exchange_active,
                            response_sender,
                        }).map_err(|_| panic!("failed to send request to B[2]"))
                            .and_then(move |tx_b| Ok((tx_a, tx_b, response_receiver)))
                    })
            })
            .and_then(move |(tx_a, tx_b, response_receiver)| {
                response_receiver
                    .map_err(|_| panic!("failed to receive response from B[2]"))
                    .and_then(move |(new_channel_info_for_b, channel_ready)| {
                        let (response_sender, response_receiver) = oneshot::channel();

                        tx_a.send(ToHandshakeManager::ChannelReady {
                            channel_ready,
                            response_sender,
                        }).map_err(|_| panic!("failed to send request to A[3]"))
                            .and_then(move |_| Ok((new_channel_info_for_b, response_receiver)))
                    })
            })
            .and_then(move |(new_channel_info_for_b, response_receiver)| {
                response_receiver
                    .map_err(|_| panic!("failed to receive response from A[3]"))
                    .and_then(move |new_channel_info_for_a| {
                        Ok((new_channel_info_for_a, new_channel_info_for_b))
                    })
            });

        let (channel_a, channel_b) = core.run(handshake_task).unwrap();

        assert_eq!(channel_a.sender_id, channel_b.receiver_id);
        assert_eq!(channel_a.receiver_id, channel_b.sender_id);

        // FIXME: We can't access the raw bytes of the SealingKey and OpeningKey
        // assert_eq!(channel_a.sender_key, channel_b.receiver_key);
        // assert_eq!(channel_a.receiver_key, channel_a.sender_key);
    }
}
