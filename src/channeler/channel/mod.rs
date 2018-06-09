use std::net::SocketAddr;
use std::convert::TryFrom;
use std::collections::HashMap;
use std::collections::VecDeque;

use bytes::{Bytes};
use byteorder::{ByteOrder, LittleEndian};

use utils::WindowNonce;
use crypto::identity::PublicKey;
use proto::{
    Proto,
    ProtoError,
    channeler::{ChannelId, CHANNEL_ID_LEN, Plain, ChannelerMessage}
};

use channeler::handshake::HandshakeResult;
use channeler::config::{CHANNEL_KEEPALIVE_TIMEOUT, MAXIMUM_CAROUSEL_RECEIVER};

const TAG_LEN: usize = 16;
const NONCE_LEN: usize = 12;
const MINIMUM_MESSAGE_LENGTH: usize = CHANNEL_ID_LEN + NONCE_LEN + TAG_LEN;

mod tx;
mod rx;

use self::tx::Tx;
use self::rx::Rx;

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

pub struct Channel {
    tx: Option<Tx>,
    carousel_rx: VecDeque<Rx>,
}

pub struct ChannelPool {
    channels: HashMap<PublicKey, Channel>,
    idx_id: HashMap<ChannelId, PublicKey>,
}

#[derive(Debug)]
pub enum ChannelError {
    InvalidConfig,

    /// Message too short, we discard the message.
    MessageTooShort,

    /// Error may occur in sealing plain message.
    EncryptFailed,

    /// Error may occur in opening sealed message.
    DecryptFailed,

    TxNotFound,

    /// When we received a message while there is no channel matches its
    /// `ChannelId`, we report this error, `Channeler` **SHOULD** send a
    /// `UnknownChannel` message to the sender of this message.
    UnknownChannel(ChannelId),

    InvalidChannelId,

    InvalidNonce,

    Proto(ProtoError),

    // Indicate that a specified channel does not exist.
    // Unknown Channel
    NoSuchChannel,
}

impl Channel {
    pub fn new(tx: Tx, rx: Rx) -> Channel {
        let mut carousel_rx = VecDeque::with_capacity(MAXIMUM_CAROUSEL_RECEIVER);
        carousel_rx.push_back(rx);

        Channel { tx: Some(tx), carousel_rx }
    }

    fn encrypt_message(&mut self, msg: Plain)
        -> Result<(SocketAddr, Bytes), ChannelError>
    {
        self.tx_mut().and_then(move |tx| {
            tx.encrypt(msg).and_then(|encrypted| {
                Ok((tx.remote_addr(), encrypted))
            })
        })
    }

    fn decrypt_message(&mut self, channel_id: ChannelId, msg: Bytes)
        -> Result<Option<Bytes>, ChannelError>
    {
        self.rx_mut(channel_id)
            .and_then(move |rx| rx.decrypt(msg))
    }

    #[inline]
    fn tx_mut(&mut self) -> Result<&mut Tx, ChannelError> {
        self.tx.as_mut().ok_or(ChannelError::TxNotFound)
    }

    #[inline]
    fn rx_mut(&mut self, channel_id: ChannelId) -> Result<&mut Rx, ChannelError> {
        for rx in &mut self.carousel_rx {
            if rx.channel_id() == &channel_id {
                return Ok(rx)
            }
        }

        Err(ChannelError::UnknownChannel(channel_id))
    }

    #[inline]
    fn can_send_message(&self) -> bool {
        self.tx.is_some()
    }

    fn replace(&mut self, tx: Tx, rx: Rx) -> Option<Rx> {
        self.tx = Some(tx);
        self.carousel_rx.push_back(rx);

        if self.carousel_rx.len() <= MAXIMUM_CAROUSEL_RECEIVER {
            None
        } else {
            self.carousel_rx.pop_front()
        }
    }
}

impl ChannelPool {
    pub fn add_channel(&mut self, addr: SocketAddr, info: HandshakeResult) {
        let tx = Tx::new(addr, info.channel_tx_id, info.channel_tx_key);
        let rx = Rx::new(info.channel_rx_id, info.channel_rx_key);

        if let Some(cur_channel) = self.channels.get_mut(&info.remote_public_key) {
            self.idx_id.insert(rx.channel_id().clone(), info.remote_public_key);
            let expired_rx = cur_channel.replace(tx, rx);

            if let Some(rx) = expired_rx {
                self.idx_id.remove(rx.channel_id());
            }
        } else {
            self.idx_id.insert(rx.channel_id().clone(), info.remote_public_key.clone());

            let new_channel = Channel::new(tx, rx);
            self.channels.insert(info.remote_public_key, new_channel);
        }
    }

    // XXX: used to determind whether we should reconnect,
    // rename to `connected_to(xxx)`?
    #[inline]
    pub fn contains_neighbor(&self, pk: &PublicKey) -> bool {
        self.channels.contains_key(pk)
    }

    pub fn remove_neighbor(&mut self, public_key: &PublicKey) {
        if let Some(channel) = self.channels.remove(public_key) {
            for rx in channel.carousel_rx {
                let _ = self.idx_id.remove(rx.channel_id());
            }
        }
    }

    pub fn encrypt_message(
        &mut self,
        public_key: &PublicKey,
        plain: Plain
    ) -> Result<(SocketAddr, Bytes), ChannelError> {
        let channel = self.channels.get_mut(public_key)
            .ok_or(ChannelError::NoSuchChannel)?; // FIXME: use Disconnected ?

        channel.encrypt_message(plain).and_then(|(remote_addr, encrypted)| {
            ChannelerMessage::Encrypted(encrypted)
                .encode()
                .map_err(ChannelError::Proto)
                .and_then(move |message| Ok((remote_addr, message)))
        })
    }

    pub fn decrypt_message(&mut self, mut encrypted: Bytes)
        -> Result<(PublicKey, Option<Bytes>), ChannelError>
    {
        if encrypted.len() <= MINIMUM_MESSAGE_LENGTH {
            return Err(ChannelError::MessageTooShort);
        }

        let channel_id = ChannelId::try_from(&encrypted.split_to(CHANNEL_ID_LEN))
            .map_err(|_| ChannelError::InvalidChannelId)?;

        match self.idx_id.get(&channel_id) {
            None => Err(ChannelError::UnknownChannel(channel_id)),
            Some(pk) => {
                let channel = self.channels.get_mut(pk).expect("index broken");
                channel.decrypt_message(channel_id, encrypted)
                    .and_then(|message| Ok((pk.clone(), message)))
            }
        }
    }

    pub fn time_tick(&mut self) -> Result<Vec<PublicKey>, ChannelError> {
        let mut should_send_keepalive = Vec::new();

        for (public_key, channel) in &mut self.channels {
            for rx in &mut channel.carousel_rx {
                if rx.keepalive_timeout > 0 {
                    rx.keepalive_timeout -= 1;
                } else {
                    self.idx_id.remove(rx.channel_id());
                }
            }

            // If the latest receiving end expired, remove relevant sending end.
            if let Some(latest_rx) = channel.carousel_rx.back() {
                if latest_rx.keepalive_timeout == 0 {
                    channel.tx = None;
                }
            }

            // Remove expired receiving ends.
            channel.carousel_rx.retain(|rx| rx.keepalive_timeout > 0);

            if let Some(tx) = channel.tx.as_mut() {
                if tx.keepalive_timeout > 0 {
                    tx.keepalive_timeout -= 1;
                } else {
                    should_send_keepalive.push(public_key.clone());
                    tx.keepalive_timeout = CHANNEL_KEEPALIVE_TIMEOUT;
                }
            }
        }

        self.channels.retain(|_, channel| !channel.carousel_rx.is_empty());

        Ok(should_send_keepalive)
    }
}
