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

const MINIMUM_MESSAGE_LENGTH: usize = CHANNEL_ID_LEN + NONCE_LEN + TAG_LEN;

const MAX_RECEIVING_END: usize = 3;
const NONCE_WINDOW_LEN: usize = 256;
const CHANNEL_KEEPALIVE_TIMEOUT: usize = 200;

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

pub struct SendingEnd {
    sender_channel_id: ChannelId,

    remote_addr: SocketAddr,
    nonce: Nonce,
    sealing_key: SealingKey,

    keepalive_timeout: usize,
}

impl SendingEnd {
    fn encrypt(&mut self, plain: Plain) -> Result<Bytes, ()> {
        static PREFIX_LENGTH: usize = CHANNEL_ID_LEN + NONCE_LEN;

        let serialized = plain.encode().map_err(|e| {
            error!("failed to encode plain message {:?}", e)
        })?;

        let mut buf =
            BytesMut::with_capacity(PREFIX_LENGTH + serialized.len() + TAG_LEN);

        buf.extend_from_slice(&self.sender_channel_id);
        buf.extend_from_slice(&self.nonce);
        buf.extend_from_slice(&serialized);
        buf.extend_from_slice(&[0x00; TAG_LEN][..]);

        seal_in_place(&self.sealing_key, &self.nonce,
            &self.sender_channel_id,
            &mut buf[PREFIX_LENGTH..], TAG_LEN
        ).map_err(|e| {
            error!("failed to seal plain message {:?}", e);
        }).and_then(|out_len| {
            increase_nonce(&mut self.nonce);
            Ok(buf.split_to(PREFIX_LENGTH + out_len).freeze())
        })
    }

//    fn encrypt(&mut self, content: Option<Bytes>, rng: &SecureRandom) -> Result<Bytes, ()> {
//        static PREFIX_LENGTH: usize = CHANNEL_ID_LEN + NONCE_LEN;
//
//        let plain_content = match content {
//            None => PlainContent::KeepAlive,
//            Some(app_data) => PlainContent::Application(app_data),
//        };
//
//        let rand_padding = generate_random_bytes(MAX_RAND_PADDING_LEN, rng)
//            .map_err(|e| error!("failed to generate padding bytes {:?}", e))?;
//
//        let serialized = Plain { rand_padding, content: plain_content }
//            .encode()
//            .map_err(|e| error!("failed to encode plain message {:?}", e))?;
//
//        let mut buf =
//            BytesMut::with_capacity(PREFIX_LENGTH + serialized.len() + TAG_LEN);
//
//        buf.extend_from_slice(&self.sender_channel_id);
//        buf.extend_from_slice(&self.nonce);
//        buf.extend_from_slice(&serialized);
//        buf.extend_from_slice(&[0x00; TAG_LEN][..]);
//
//        seal_in_place(&self.sealing_key, &self.nonce,
//            &self.sender_channel_id,
//            &mut buf[PREFIX_LENGTH..], TAG_LEN
//        ).map_err(|e| {
//            error!("failed to seal plain message {:?}", e);
//        }).and_then(|out_len| {
//            increase_nonce(&mut self.nonce);
//            Ok(buf.split_to(PREFIX_LENGTH + out_len).freeze())
//        })
//    }
}

pub struct ReceivingEnd {
    receiver_channel_id: ChannelId,

    recv_window: NonceWindow,
    opening_key: OpeningKey,

    keepalive_timeout: usize,
}

impl ReceivingEnd {
    fn decrypt(&mut self, mut encrypted: Bytes) -> Result<Option<Bytes>, ()> {
        let nonce = Nonce::try_from(&encrypted.split_to(NONCE_LEN)).unwrap();

        open_in_place(&self.opening_key, &nonce,
            &self.receiver_channel_id, 0,
            &mut BytesMut::from(encrypted)
        ).map_err(|e| {
            error!("failed to open sealed message {:?}", e);
        }).and_then(|serialized_plain| {
            Plain::decode(serialized_plain).map_err(|e| {
                error!("failed to decode serialized plain message: {:?}", e);
            }).and_then(|plain| {
                match plain.content {
                    PlainContent::Application(app_data) => Ok(Some(app_data)),
                    PlainContent::KeepAlive => {
                        self.keepalive_timeout = 2 * CHANNEL_KEEPALIVE_TIMEOUT;
                        Ok(None)
                    }
                }
            })
        })
    }
}

pub struct CarouselReceivingEnd {
    inner: VecDeque<ReceivingEnd>,
    capacity: usize,
}

impl CarouselReceivingEnd {
    pub fn new(capacity: usize) -> CarouselReceivingEnd {
        CarouselReceivingEnd {
            inner: VecDeque::with_capacity(capacity),

            capacity,
        }
    }

    pub fn push(&mut self, value: ReceivingEnd) {
        while self.inner.len() >= self.capacity {
            self.inner.pop_front();
        }

        self.inner.push_back(value);
    }

    pub fn contains(&self, channel_id: &ChannelId) -> bool {
        self.inner.iter().fold(false, |found, receiving_end| {
            found || receiving_end.receiver_channel_id == *channel_id
        })
    }

    pub fn decrypt(&mut self, channel_id: ChannelId, mut encrypted: Bytes) -> Result<Option<Bytes>, ()> {
        assert!(encrypted.len() < NONCE_LEN + TAG_LEN, "message too short");

        for receiving_end in self.inner.iter_mut() {
            if receiving_end.receiver_channel_id == channel_id {
                return receiving_end.decrypt(encrypted);
            }
        }

        unreachable!("not contain specified channel")
    }

    #[inline]
    fn ids(&self) -> Vec<&ChannelId> {
        self.inner.iter().map(|receiving_end| {
            &receiving_end.receiver_channel_id
        }).collect()
    }
}

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
        let sending_end = SendingEnd {
            remote_addr: addr,
            sender_channel_id: info.sender_id,
            sealing_key: info.sender_key,
            nonce: Nonce::default(),
            keepalive_timeout: CHANNEL_KEEPALIVE_TIMEOUT,
        };

        let receiving_end = ReceivingEnd {
            receiver_channel_id: info.receiver_id,
            recv_window: NonceWindow::new(NONCE_WINDOW_LEN),
            opening_key: info.receiver_key,
            keepalive_timeout: 2 * CHANNEL_KEEPALIVE_TIMEOUT,
        };

        if let Some(channel) = self.inner.get_mut(&info.remote_public_key) {
            self.index.insert(
                receiving_end.receiver_channel_id.clone(),
                info.remote_public_key
            );

            channel.sending_end = Some(sending_end);
            channel.carousel_receiving_end.push(receiving_end);
        } else {
            self.index.insert(
                receiving_end.receiver_channel_id.clone(),
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
        let mut sending_end = self.inner.get_mut(public_key)
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

        Ok((sending_end.remote_addr, message))
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

#[cfg(test)]
mod tests {
    use super::*;

    use ring::aead::CHACHA20_POLY1305;

//    #[test]
//    fn test_encrypt_decrypt() {
//        let mut tx = SendingEnd {
//            remote_addr: "127.0.0.1:10001".parse().unwrap(),
//            sender_channel_id: ChannelId::from(&[0x00; CHANNEL_ID_LEN]),
//            nonce: Nonce::default(),
//            sealing_key: SealingKey::new(&CHACHA20_POLY1305, &[0x01; 32][..]).unwrap(),
//            keepalive_timeout: 100,
//        };
//
//        let mut rx = ReceivingEnd {
//            receiver_channel_id: ChannelId::from(&[0x00; CHANNEL_ID_LEN]),
//            recv_window: NonceWindow::new(256),
//            opening_key: OpeningKey::new(&CHACHA20_POLY1305, &[0x01; 32][..]).unwrap(),
//            keepalive_timeout: 200,
//        };
//
//        let plain1 = Plain {
//            rand_padding: Bytes::from(vec![0xff; 32]),
//            content: PlainContent::Application(Bytes::from("hello!")),
//        };
//
//        let mut encrypted = encrypt(&mut tx, plain1).unwrap();
//
//        let _channel_id = encrypted.split_to(CHANNEL_ID_LEN);
//
//        let plain2 = decrypt(&mut rx, encrypted.clone()).unwrap();
//
//        assert_eq!(plain2.content, PlainContent::Application(Bytes::from("hello!")));
//
//        // Don't accept a same message twice.
//        assert!(decrypt(&mut rx, encrypted).is_err());
//    }
}
