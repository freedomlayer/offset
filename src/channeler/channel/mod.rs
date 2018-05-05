#![allow(unused)] // FIXME: remove this line

use std::net::SocketAddr;
use std::convert::TryFrom;
use std::collections::{HashMap};

use bytes::{Bytes};
use byteorder::{ByteOrder, LittleEndian};

use crypto::identity::PublicKey;
use proto::{Proto, ProtoError};
use proto::channeler::{ChannelId, CHANNEL_ID_LEN, Plain, ChannelerMessage};
use utils::WindowNonce;

use channeler::handshake::HandshakeResult;

const TAG_LEN: usize = 16;
const NONCE_LEN: usize = 12;

const MINIMUM_MESSAGE_LENGTH: usize = CHANNEL_ID_LEN + NONCE_LEN + TAG_LEN;

const MAX_RECEIVING_END: usize = 3;
const NONCE_WINDOW_LEN: usize = 256;
const CHANNEL_KEEPALIVE_TIMEOUT: usize = 200;

mod sending_end;
mod receiving_end;

use self::sending_end::SendingEnd;
use self::receiving_end::{ReceivingEnd, CarouselReceivingEnd};

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
    sending_end: Option<SendingEnd>,
    carousel_receiving_end: CarouselReceivingEnd,
}

pub struct ChannelPool {
    inner: HashMap<PublicKey, Channel>,
    index: HashMap<ChannelId, PublicKey>,
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
    pub fn insert(&mut self, addr: SocketAddr, info: HandshakeResult) {
        let sending_end = SendingEnd::new(addr, info.sender_id, info.sender_key);
        let receiving_end = ReceivingEnd::new(info.receiver_id, info.receiver_key);

        if let Some(channel) = self.inner.get_mut(&info.remote_public_key) {
            self.index.insert(
                receiving_end.channel_id().clone(),
                info.remote_public_key
            );

            channel.sending_end = Some(sending_end);
            channel.carousel_receiving_end.push(receiving_end);
        } else {
            self.index.insert(
                receiving_end.channel_id().clone(),
                info.remote_public_key.clone(),
            );

            let mut carousel_receiving_end =
                CarouselReceivingEnd::new(MAX_RECEIVING_END);
            carousel_receiving_end.push(receiving_end);

            let new_channel = Channel {
                sending_end: Some(sending_end),
                carousel_receiving_end,
            };
            self.inner.insert(info.remote_public_key, new_channel);
        }
    }

    pub fn remove(&mut self, public_key: &PublicKey) {
        if let Some(channel) = self.inner.remove(public_key) {
            for channel_id in channel.carousel_receiving_end.ids() {
                let _ = self.index.remove(channel_id);
            }
        }
    }

    pub fn encrypt_outgoing(&mut self, public_key: &PublicKey, plain: Plain)
        -> Result<(SocketAddr, Bytes), ChannelPoolError>
    {
        let sending_end = self.inner.get_mut(public_key)
            .ok_or(ChannelPoolError::NoSuchChannel)
            .and_then(|channel| {
                channel.sending_end.as_mut()
                    .ok_or(ChannelPoolError::NoSuchChannel)
            })?;

        let encrypted = sending_end.encrypt(plain)
            .map_err(|_| ChannelPoolError::SealingFailed)?;

        let message = ChannelerMessage::Encrypted(encrypted)
            .encode()
            .map_err(ChannelPoolError::Proto)?;

        Ok((sending_end.remote_addr(), message))
    }

    pub fn decrypt_incoming(&mut self, mut encrypted: Bytes)
        -> Result<(PublicKey, Option<Bytes>), ChannelPoolError>
    {
        if encrypted.len() <= CHANNEL_ID_LEN + NONCE_LEN + TAG_LEN {
            return Err(ChannelPoolError::InvalidMessage);
        }

        let channel_id = ChannelId::try_from(&encrypted.split_to(CHANNEL_ID_LEN))
            .map_err(|_| ChannelPoolError::InvalidChannelId)?;

        let public_key = self.index.get(&channel_id).ok_or(ChannelPoolError::NoSuchChannel)?;
        let channel = self.inner.get_mut(public_key).expect("channel pool index broken");

        let opt_content = channel.carousel_receiving_end.decrypt(channel_id, encrypted)
            .map_err(|_| ChannelPoolError::OpeningFailed)?;

        Ok((public_key.clone(), opt_content))
    }

    pub fn time_tick(&mut self) -> Result<Vec<PublicKey>, ChannelPoolError> {
        unimplemented!()
//        let mut should_send_keepalive = Vec::new();
//
//        for (public_key, channel) in self.inner.iter_mut() {
//            if let Some(newest_tx) = channel.sending_end.as_mut() {
//                let newest_rx = channel.rx.back_mut()
//                    .expect("no receiving end, while there is a sending end!");
//
//                if newest_rx.keepalive_timeout > 0 {
//                    newest_rx.keepalive_timeout -= 1;
//
//                    // Check whether we need to send keepalive message
//                    if newest_tx.keepalive_timeout == 0 {
//                        should_send_keepalive.push(public_key.clone());
//                        newest_tx.keepalive_timeout = self.config.keepalive_timeout;
//                    } else {
//                        newest_tx.keepalive_timeout -= 1;
//                    }
//                }
//            }
//        }
//
//        Ok(should_send_keepalive)
    }
}
