use std::collections::HashMap;
use std::collections::VecDeque;
use std::convert::TryFrom;
use std::net::SocketAddr;

use bytes::{Bytes, BytesMut};
use byteorder::{ByteOrder, LittleEndian};
use ring::aead::{SealingKey, OpeningKey, seal_in_place, open_in_place};

use crypto::identity::PublicKey;
use proto::{Schema, SchemaError};
use proto::channeler_udp::{Plain, PlainContent, ChannelerMessage};
use utils::{NonceWindow, WindowNonce};

const TAG_LEN: usize = 16;
const NONCE_LEN: usize = 12;
pub const CHANNEL_ID_LEN: usize = 8;

define_wrapped_bytes!(Nonce, NONCE_LEN);
define_wrapped_bytes!(ChannelId, CHANNEL_ID_LEN);

impl<'a> From<&'a Nonce> for u128 {
    #[inline]
    fn from(nonce: &'a Nonce) -> u128 {
        let mut aligned = Vec::from(&nonce.0[..]);
        aligned.resize(16, 0);

        LittleEndian::read_u128(&aligned)
    }
}

impl<'a> WindowNonce for &'a Nonce {}

// Cleanup: Find a proper place for this func and make it public.
/// Increase the bytes represented number by 1.
///
/// Reference: `libsodium/sodium/utils.c#L241`
#[inline]
fn increase_nonce(nonce: &mut [u8]) {
    let mut c: u16 = 1;
    for i in nonce {
        c += u16::from(*i);
        *i = c as u8;
        c >>= 8;
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct ChannelPoolConfig {
    pub keepalive_ticks:       usize,
    pub maximum_receiver:      usize,
    pub receiving_window_size: usize,
}

impl Default for ChannelPoolConfig {
    fn default() -> ChannelPoolConfig {
        ChannelPoolConfig {
            keepalive_ticks:       100,
            maximum_receiver:      3,
            receiving_window_size: 256,
        }
    }
}

impl ChannelPoolConfig {
    // TODO: Read configuration from file?

    /// Check if the configuration is valid.
    pub fn is_valid(&self) -> bool {
        self.maximum_receiver           >= 3 &&
        self.receiving_window_size % 64 == 0
    }
}

pub struct NewChannelInfo {
    pub sender_id:    ChannelId,
    pub sender_key:   SealingKey,
    pub receiver_id:  ChannelId,
    pub receiver_key: OpeningKey,

    pub remote_public_key: PublicKey,
}

pub struct SenderInfo {
    addr:  SocketAddr,
    id:    ChannelId,
    nonce: Nonce,
    key:   SealingKey,

    keepalive_ticks: usize,
}

pub struct ReceiverInfo {
    id:  ChannelId,
    wnd: NonceWindow,
    key: OpeningKey,

    keepalive_ticks: usize,
}

pub struct Channel {
    tx: Option<SenderInfo>,
    rx: VecDeque<ReceiverInfo>,
}

pub struct ChannelPool {
    inner: HashMap<PublicKey, Channel>,

    index: HashMap<ChannelId, PublicKey>,

    config: ChannelPoolConfig,
}

#[derive(Debug)]
pub enum ChannelPoolError {
    InvalidConfig,

    InvalidChannelId,

    InvalidNonce,

    SealingFailed,

    OpeningFailed,

    Schema(SchemaError),

    // Indicate that a specified channel does not exist.
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
                inner:  HashMap::new(),
                index:  HashMap::new(),
                config: config,
            })
        }
    }

    /// Add a "new" channel to the pool.
    pub fn add_channel(&mut self, addr: SocketAddr, info: NewChannelInfo) {
        // Build sender and receiver
        let tx = SenderInfo {
            addr,
            id: info.sender_id,
            nonce: Nonce::zero(),
            key: info.sender_key,
            keepalive_ticks: self.config.keepalive_ticks,
        };
        let rx = ReceiverInfo {
            id: info.receiver_id,
            wnd: NonceWindow::new(self.config.receiving_window_size),
            key: info.receiver_key,
            keepalive_ticks: 2 * self.config.keepalive_ticks,
        };

        let public_key = info.remote_public_key;

        if let Some(channel) = self.inner.get_mut(&public_key) {
            // Remove the old receiving ends
            while channel.rx.len() >= self.config.maximum_receiver {
                let old_rx = channel.rx.pop_front().unwrap();
                self.index.remove(&old_rx.id);
            }
            // Push the new receiving end to the ring
            self.index.insert(rx.id.clone(), public_key);
            channel.rx.push_back(rx);

            // Replace the old sending ends
            channel.tx = Some(tx);
        } else {
            self.index.insert(rx.id.clone(), public_key.clone());
            let new_channel = Channel {
                tx: Some(tx),
                rx: VecDeque::from(vec![rx]),
            };
            self.inner.insert(public_key, new_channel);
        }
    }

    /// Delete a channel from pool.
    pub fn del_channel(&mut self, public_key: &PublicKey) {
        if let Some(channel) = self.inner.remove(public_key) {
            for rx in channel.rx {
                let _ = self.index.remove(&rx.id);
            }
        }
    }

    pub fn encrypt_outgoing_msg(
        &mut self,
        public_key: &PublicKey,
        msg: Plain
    ) -> Result<(SocketAddr, Bytes), ChannelPoolError> {
        let mut tx = self.inner.get_mut(public_key).ok_or(ChannelPoolError::NoSuchChannel)
            .and_then(|ch| ch.tx.as_mut().ok_or(ChannelPoolError::NoSuchChannel))?;

        let encrypted = encrypt(&mut tx, msg)?;
        let msg = ChannelerMessage::Encrypted(encrypted).encode().map_err(ChannelPoolError::Schema)?;

        Ok((tx.addr, msg))
    }

    pub fn process_incoming_msg(&mut self, mut msg: Bytes) -> Result<Option<(PublicKey, Bytes)>, ChannelPoolError> {
        let channel_id = ChannelId::try_from(msg.split_to(CHANNEL_ID_LEN).as_ref())
            .map_err(|_| ChannelPoolError::InvalidChannelId)?;

        let public_key = self.index.get(&channel_id).ok_or(ChannelPoolError::NoSuchChannel)?;

        // Get the exact channel and process the encrypted message
        if let Some(channel) = self.inner.get_mut(public_key) {
            // Search the matching sender and try to decrypt the message
            for receiver in channel.rx.iter_mut() {
                if receiver.id == channel_id {
                    // Try to decrypt this message using the given receiver,
                    // the receiving window will be update when successful.
                    let plain_content = decrypt(receiver, msg)?.content;

                    match plain_content {
                        PlainContent::KeepAlive => {
                            // Reset the keepalive counter
                            receiver.keepalive_ticks = 2 * self.config.keepalive_ticks;
                            return Ok(None)
                        }
                        PlainContent::User(content) => return Ok(Some((public_key.clone(), content))),
                    }
                }
            }
        }

        // Reach here means the index broken
        // panic!("ChannelPool: invalid index");
        // TODO: Add method to rebuild the index

        Err(ChannelPoolError::NoSuchChannel)
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
            if let Some(mut sender) = channel.tx.take() {
                // Check the keepalive_ticks of the newest receiving end first
                if let Some(receiver) = channel.rx.back_mut() {
                    if receiver.keepalive_ticks > 0 {
                        receiver.keepalive_ticks -= 1;

                        // Check if we need to send keepalive message
                        if sender.keepalive_ticks == 0 {
                            should_send_keepalive.push(public_key.clone());
                            // Reset the keepalive tick counter
                            sender.keepalive_ticks = self.config.keepalive_ticks;
                        } else {
                            sender.keepalive_ticks -= 1;
                        }

                        // Put the sender back
                        channel.tx = Some(sender);
                    }
                } else {
                    panic!("ChannelPool: internal state inconsistent");
                }
            }
        }

        Ok(should_send_keepalive)
    }
}

/// Decrypt a encrypted serialized `Plain` message, returns the `Plain` message
/// on success. On failure, returns an error indicates the reason.
#[inline]
fn decrypt(receiver: &mut ReceiverInfo, mut encrypted: Bytes) -> Result<Plain, ChannelPoolError> {
    // FIXME: will panic if len(encrypted) < NONCE_LEN
    let nonce = Nonce::try_from(encrypted.split_to(NONCE_LEN).as_ref())
        .map_err(|_| ChannelPoolError::InvalidNonce)?;


    let ad = &receiver.id;
    let key = &receiver.key;

    match open_in_place(key, &nonce, ad, 0, &mut BytesMut::from(encrypted)) {
        Ok(serialized_plain) => {
            let plain = Plain::decode(serialized_plain).map_err(ChannelPoolError::Schema)?;

            if receiver.wnd.try_accept(&nonce) {
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
fn encrypt(sender: &mut SenderInfo, plain: Plain) -> Result<Bytes, ChannelPoolError> {
    static PREFIX_LENGTH: usize = CHANNEL_ID_LEN + NONCE_LEN;

    let serialized = plain.encode().map_err(ChannelPoolError::Schema)?;

    let mut buf = BytesMut::with_capacity(PREFIX_LENGTH + serialized.len() + TAG_LEN);

    buf.extend_from_slice(&sender.id);
    buf.extend_from_slice(&sender.nonce);
    buf.extend_from_slice(&serialized);
    buf.extend_from_slice(&[0x00; TAG_LEN][..]);

    let ad = &sender.id;
    let key = &sender.key;
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
    use proto::channeler_udp::PlainContent;

    use ring::aead::CHACHA20_POLY1305;

    #[test]
    fn test_encrypt_decrypt() {
        let mut tx = SenderInfo {
            addr: "127.0.0.1:10001".parse().unwrap(),
            id: ChannelId::try_from(&[0x00; CHANNEL_ID_LEN][..]).unwrap(),
            nonce: Nonce::zero(),
            key: SealingKey::new(&CHACHA20_POLY1305, &[0x01; 32][..]).unwrap(),
            keepalive_ticks: 100,
        };

        let mut rx = ReceiverInfo {
            id: ChannelId::try_from(&[0x00; CHANNEL_ID_LEN][..]).unwrap(),
            wnd: NonceWindow::new(256),
            key: OpeningKey::new(&CHACHA20_POLY1305, &[0x01; 32][..]).unwrap(),
            keepalive_ticks: 200,
        };

        let plain1 = Plain {
            rand_padding: Bytes::from(vec![0xff; 32]),
            content: PlainContent::User(Bytes::from("hello!")),
        };

        let mut encrypted = encrypt(&mut tx, plain1).unwrap();

        let _channel_id = encrypted.split_to(CHANNEL_ID_LEN);

        let plain2 = decrypt(&mut rx, encrypted.clone()).unwrap();

        assert_eq!(plain2.content, PlainContent::User(Bytes::from("hello!")));

        // Don't accept a same message twice.
        assert!(decrypt(&mut rx, encrypted).is_err());
    }

    #[test]
    fn test_channel_pool() {}
}

