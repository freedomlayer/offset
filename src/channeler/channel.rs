use std::net::SocketAddr;
use std::convert::TryFrom;
use std::collections::{HashMap, VecDeque};

use bytes::{Bytes, BytesMut};
use byteorder::{ByteOrder, LittleEndian};
use ring::aead::{SealingKey, OpeningKey, seal_in_place, open_in_place};

use crypto::{increase_nonce, identity::PublicKey};
use proto::{Proto, ProtoError};
use proto::channeler::{ChannelId, CHANNEL_ID_LEN, Plain, PlainContent, ChannelerMessage};
use utils::{NonceWindow, WindowNonce};

use channeler::handshake::HandshakeResult;

const TAG_LEN: usize = 16;
const NONCE_LEN: usize = 12;

define_fixed_bytes!(Nonce, NONCE_LEN);

impl<'a> From<&'a Nonce> for u128 {
    #[inline]
    fn from(nonce: &'a Nonce) -> u128 {
        let mut aligned = Vec::from(&nonce.0[..]);
        aligned.resize(16, 0);

        LittleEndian::read_u128(&aligned)
    }
}

impl<'a> WindowNonce for &'a Nonce {}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct ChannelPoolConfig {
    pub keepalive_timeout: usize,
    pub max_receiving_end: usize,
    pub receiving_window_size: usize,
}

impl Default for ChannelPoolConfig {
    fn default() -> ChannelPoolConfig {
        ChannelPoolConfig {
            keepalive_timeout: 100,
            max_receiving_end: 3,
            receiving_window_size: 256,
        }
    }
}

impl ChannelPoolConfig {
    // TODO: Read configuration from file?

    /// Check if the configuration is valid.
    pub fn is_valid(&self) -> bool {
        self.max_receiving_end >= 3 &&
            self.receiving_window_size % 64 == 0
    }
}

pub struct SenderState {
    channel_id: ChannelId,
    remote_addr: SocketAddr,
    nonce: Nonce,
    sealing_key: SealingKey,

    keepalive_timeout: usize,
}

pub struct ReceiverState {
    channel_id: ChannelId,
    recv_window: NonceWindow,
    opening_key: OpeningKey,

    keepalive_timeout: usize,
}

pub struct Channel {
    tx: Option<SenderState>,
    rx: VecDeque<ReceiverState>,
}

pub struct ChannelPool {
    inner: HashMap<PublicKey, Channel>,
    index: HashMap<ChannelId, PublicKey>,

    config: ChannelPoolConfig,
}

#[derive(Debug)]
pub enum ChannelPoolError {
    InvalidConfig,

    InvalidMessage,

    InvalidChannelId,

    InvalidNonce,

    SealingFailed,

    OpeningFailed,

    Proto(ProtoError),

    // Indicate that a specified channel does not exist.
    // Unknown Channel
    NoSuchChannel,
}

impl ChannelPool {
    /// Creates a new `ChannelPool` with the default `ChannelPoolConfig`.
    pub fn new() -> Result<ChannelPool, ChannelPoolError> {
        ChannelPool::with_config(ChannelPoolConfig::default())
    }

    /// Creates a new `ChannelPool` with the specified `ChannelPoolConfig`.
    pub fn with_config(config: ChannelPoolConfig) -> Result<ChannelPool, ChannelPoolError> {
        if !config.is_valid() {
            Err(ChannelPoolError::InvalidConfig)
        } else {
            Ok(ChannelPool {
                inner: HashMap::new(),
                index: HashMap::new(),

                config,
            })
        }
    }

    /// Add a "new" channel to the pool.
    pub fn insert(&mut self, addr: SocketAddr, info: HandshakeResult) {
        // Build sender and receiver
        let tx = SenderState {
            remote_addr: addr,
            channel_id: info.sender_id,
            sealing_key: info.sender_key,
            nonce: Nonce::default(),
            keepalive_timeout: self.config.keepalive_timeout,
        };
        let rx = ReceiverState {
            channel_id: info.receiver_id,
            recv_window: NonceWindow::new(self.config.receiving_window_size),
            opening_key: info.receiver_key,
            keepalive_timeout: 2 * self.config.keepalive_timeout,
        };

        let public_key = info.remote_public_key;

        if let Some(channel) = self.inner.get_mut(&public_key) {
            // Remove the old receiving ends
            while channel.rx.len() >= self.config.max_receiving_end {
                let old_rx = channel.rx.pop_front().unwrap();
                self.index.remove(&old_rx.channel_id);
            }
            // Push the new receiving end to the ring
            self.index.insert(rx.channel_id.clone(), public_key);
            channel.rx.push_back(rx);

            // Replace the old sending ends
            channel.tx = Some(tx);
        } else {
            self.index.insert(rx.channel_id.clone(), public_key.clone());
            let new_channel = Channel {
                tx: Some(tx),
                rx: VecDeque::from(vec![rx]),
            };
            self.inner.insert(public_key, new_channel);
        }
    }

    /// Delete a channel from pool.
    pub fn remove(&mut self, public_key: &PublicKey) {
        if let Some(channel) = self.inner.remove(public_key) {
            for rx in channel.rx {
                let _ = self.index.remove(&rx.channel_id);
            }
        }
    }

    pub fn encrypt_outgoing(
        &mut self,
        public_key: &PublicKey,
        plain: Plain
    ) -> Result<(SocketAddr, Bytes), ChannelPoolError> {
        let mut tx = self.inner.get_mut(public_key).ok_or(ChannelPoolError::NoSuchChannel)
            .and_then(|ch| ch.tx.as_mut().ok_or(ChannelPoolError::NoSuchChannel))?;

        let encrypted = encrypt(&mut tx, plain)?;
        let msg = ChannelerMessage::Encrypted(encrypted).encode().map_err(ChannelPoolError::Proto)?;

        Ok((tx.remote_addr, msg))
    }

    pub fn decrypt_incoming(
        &mut self,
        mut encrypted: Bytes
    ) -> Result<Option<(PublicKey, Bytes)>, ChannelPoolError> {
        if encrypted.len() <= CHANNEL_ID_LEN + NONCE_LEN + TAG_LEN {
            Err(ChannelPoolError::InvalidMessage)
        } else {
            let channel_id = ChannelId::try_from(encrypted.split_to(CHANNEL_ID_LEN).as_ref())
                .map_err(|_| ChannelPoolError::InvalidChannelId)?;

            // Get the key of the two-level index
            let public_key = self.index.get(&channel_id).ok_or(ChannelPoolError::NoSuchChannel)?;

            // Get the exact channel and process the encrypted message
            if let Some(channel) = self.inner.get_mut(public_key) {
                // Search the matching sender and try to decrypt the message
                for rx in channel.rx.iter_mut() {
                    if rx.channel_id == channel_id {
                        // Try to decrypt this message using the given receiver,
                        // the receiving window will be update when successful.
                        let plain_content = decrypt(rx, encrypted)?.content;

                        match plain_content {
                            PlainContent::KeepAlive => {
                                rx.keepalive_timeout = 2 * self.config.keepalive_timeout;
                                return Ok(None)
                            }
                            PlainContent::Application(content) => {
                                return Ok(Some((public_key.clone(), content)))
                            }
                        }
                    }
                }
            }

            // Reach here means the index broken
            warn!("index broken");
            self.rebuild_index();

            Err(ChannelPoolError::NoSuchChannel)
        }
    }

    // Handle the timer tick event, returns an array of public key, which indicates
    // we need to send keepalive to those neighbor.
    //
    // # Panicss
    //
    // Panics if the internal state inconsistent.
    pub fn process_timer_tick(&mut self) -> Result<Vec<PublicKey>, ChannelPoolError> {
        let mut should_send_keepalive = Vec::new();

        for (public_key, channel) in self.inner.iter_mut() {
            // If we have a sender in this channel, we should done the following:
            //
            // Firstly, we check whether the newest receiving end have no activity
            // for a while, if so, we remove the corresponding sending end.
            // Then we check whether we need to send keepalive message to remote.
            if let Some(mut tx) = channel.tx.take() {
                // Check the keepalive_ticks of the newest receiving end first
                if let Some(rx) = channel.rx.back_mut() {
                    if rx.keepalive_timeout > 0 {
                        rx.keepalive_timeout -= 1;

                        // Check whether we need to send keepalive message
                        if tx.keepalive_timeout == 0 {
                            should_send_keepalive.push(public_key.clone());
                            tx.keepalive_timeout = self.config.keepalive_timeout;
                        } else {
                            tx.keepalive_timeout -= 1;
                        }

                        channel.tx = Some(tx);
                    }
                } else {
                    error!("no receiving end, while there is a sending end");
                }
            }
        }

        Ok(should_send_keepalive)
    }

    fn rebuild_index(&mut self) {
        info!("rehashing index");

        self.index.clear();
        for (public_key, channel) in &self.inner {
            for rx in &channel.rx {
                self.index.insert(rx.channel_id.clone(), public_key.clone());
            }
        }

        info!("index rehashed");
    }
}

/// Decrypt a encrypted serialized `Plain` message, returns the `Plain` message
/// on success. On failure, returns an error indicates the reason.
#[inline]
fn decrypt(receiver: &mut ReceiverState, mut encrypted: Bytes) -> Result<Plain, ChannelPoolError> {
    // NOTE: The caller should promise encrypted.len() > NONCE_LEN
    debug_assert!(encrypted.len() > NONCE_LEN);

    let nonce = Nonce::try_from(encrypted.split_to(NONCE_LEN).as_ref())
        .map_err(|_| ChannelPoolError::InvalidNonce)?;


    let ad = &receiver.channel_id;
    let key = &receiver.opening_key;

    match open_in_place(key, &nonce, ad, 0, &mut BytesMut::from(encrypted)) {
        Ok(serialized_plain) => {
            let plain = Plain::decode(serialized_plain).map_err(ChannelPoolError::Proto)?;

            if receiver.recv_window.try_accept(&nonce) {
                Ok(plain)
            } else {
                Err(ChannelPoolError::InvalidNonce)
            }
        }
        Err(_) => Err(ChannelPoolError::OpeningFailed),
    }
}

/// Encrypt a `Plain` message, returns the encrypted serialized `Plain` message
/// of the input on success. On failure, returns an error indicates the reason.
#[inline]
fn encrypt(sender: &mut SenderState, plain: Plain) -> Result<Bytes, ChannelPoolError> {
    static PREFIX_LENGTH: usize = CHANNEL_ID_LEN + NONCE_LEN;

    let serialized = plain.encode().map_err(ChannelPoolError::Proto)?;

    let mut buf = BytesMut::with_capacity(PREFIX_LENGTH + serialized.len() + TAG_LEN);

    buf.extend_from_slice(&sender.channel_id);
    buf.extend_from_slice(&sender.nonce);
    buf.extend_from_slice(&serialized);
    buf.extend_from_slice(&[0x00; TAG_LEN][..]);

    let ad = &sender.channel_id;
    let key = &sender.sealing_key;
    let nonce = &sender.nonce;

    match seal_in_place(key, nonce, ad, &mut buf[PREFIX_LENGTH..], TAG_LEN) {
        Ok(out_size) => {
            increase_nonce(&mut sender.nonce);
            Ok(buf.split_to(PREFIX_LENGTH + out_size).freeze())
        }
        Err(_) => Err(ChannelPoolError::SealingFailed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use ring::aead::CHACHA20_POLY1305;

    #[test]
    fn test_encrypt_decrypt() {
        let mut tx = SenderState {
            remote_addr: "127.0.0.1:10001".parse().unwrap(),
            channel_id: ChannelId::from(&[0x00; CHANNEL_ID_LEN]),
            nonce: Nonce::default(),
            sealing_key: SealingKey::new(&CHACHA20_POLY1305, &[0x01; 32][..]).unwrap(),
            keepalive_timeout: 100,
        };

        let mut rx = ReceiverState {
            channel_id: ChannelId::from(&[0x00; CHANNEL_ID_LEN]),
            recv_window: NonceWindow::new(256),
            opening_key: OpeningKey::new(&CHACHA20_POLY1305, &[0x01; 32][..]).unwrap(),
            keepalive_timeout: 200,
        };

        let plain1 = Plain {
            rand_padding: Bytes::from(vec![0xff; 32]),
            content: PlainContent::Application(Bytes::from("hello!")),
        };

        let mut encrypted = encrypt(&mut tx, plain1).unwrap();

        let _channel_id = encrypted.split_to(CHANNEL_ID_LEN);

        let plain2 = decrypt(&mut rx, encrypted.clone()).unwrap();

        assert_eq!(plain2.content, PlainContent::Application(Bytes::from("hello!")));

        // Don't accept a same message twice.
        assert!(decrypt(&mut rx, encrypted).is_err());
    }
}
