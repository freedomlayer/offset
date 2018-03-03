use std::collections::VecDeque;
use std::convert::TryFrom;
use std::net::SocketAddr;

use bytes::{Bytes, BytesMut};
use byteorder::{ByteOrder, LittleEndian};
use ring::aead::{SealingKey, OpeningKey, seal_in_place, open_in_place};

use crypto::identity::PublicKey;
use proto::Schema;
use proto::channeler_udp::{ChannelerMessage, Plain, PlainContent};
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
pub struct ChannelConfig {
    pub max_recv_end: usize,
    pub recv_wnd_size: usize,
    pub keepalive_ticks: usize,
}

pub struct NewChannelInfo {
    pub remote_public_key: PublicKey,

    pub send_end_id:  ChannelId,
    pub send_end_key: SealingKey,
    pub recv_end_id:  ChannelId,
    pub recv_end_key: OpeningKey,
}

pub struct SendEnd {
    addr:  SocketAddr,
    id:    ChannelId,
    nonce: Nonce,
    key:   SealingKey,

    keepalive_ticks: usize,
}

pub struct RecvEnd {
    id:  ChannelId,
    wnd: NonceWindow,
    key: OpeningKey,

    keepalive_ticks: usize,
}

pub struct Channel {
    config: ChannelConfig,
    send_end: SendEnd,
    recv_ends: VecDeque<RecvEnd>,
}

/// A channel represents a symmetric channel between neighbor.
///
/// A channel works like a crypter. It contains the information needed to
/// encrypt the message needs to be sent, and determine whether we should
/// accept and decrypt a encrypted message from neighbor.
impl Channel {
    pub fn new(addr: SocketAddr, info: NewChannelInfo, config: ChannelConfig) -> Channel {
        let (send_end, recv_end) = derive_send_recv_end(addr, info, &config);

        Channel {
            config,
            send_end,
            recv_ends: VecDeque::from(vec![recv_end]),
        }
    }

    pub fn replace(&mut self, addr: SocketAddr, info: NewChannelInfo) {
        let (send_end, recv_end) = derive_send_recv_end(addr, info, &self.config);

        while self.recv_ends.len() >= self.config.max_recv_end {
            self.recv_ends.pop_front();
        }

        self.send_end = send_end;
        self.recv_ends.push_back(recv_end);
    }

    pub fn pre_send(&mut self, plain: Plain) -> Result<(SocketAddr, ChannelerMessage), ()> {
        self.encrypt(plain).and_then(|channeler_message| {
            increase_nonce(&mut self.send_end.nonce);
            Ok((self.send_end.addr, channeler_message))
        })
    }

    // TODO: Also take the addr, and update the addr when encrypt the message successful?
    pub fn try_recv(&mut self, encrypted: Bytes) -> Result<Option<Bytes>, ()> {
        let plain = self.decrypt(encrypted)?;

        match plain.content {
            PlainContent::KeepAlive => {
                // TODO: Reset the send_end keepalive_ticks if we accept this message
                // using the mapping recv_end.
                Ok(None)
            },
            PlainContent::User(content) => Ok(Some(content)),
        }
    }

    // ========== encrypt/decrypt ==========
    // FIXME: decouple?

    #[inline]
    fn encrypt(&self, plain: Plain) -> Result<ChannelerMessage, ()> {
        let serialized_plain = plain.encode().map_err(|_| ())?;
        let mut buf = BytesMut::with_capacity(NONCE_LEN + serialized_plain.len() + TAG_LEN);

        buf.extend_from_slice(&self.send_end.nonce);
        buf.extend_from_slice(&serialized_plain);
        buf.extend_from_slice(&[0x00; TAG_LEN][..]);

        let ad = &self.send_end.id;
        let key = &self.send_end.key;
        let nonce = &self.send_end.nonce;

        seal_in_place(key, nonce, ad, &mut buf[NONCE_LEN..], TAG_LEN).map_err(|_| ()).and_then(move |sz| {
            let encrypted = buf.split_to(NONCE_LEN + sz).freeze();
            Ok(ChannelerMessage::Encrypted(encrypted))
        })
    }

    #[inline]
    fn decrypt(&mut self, mut encrypted: Bytes) -> Result<Plain, ()> {
        // FIXME: Change the `Nonce` implementation to avoid copy here?
        let nonce = Nonce::try_from(encrypted.split_to(NONCE_LEN).as_ref())?;

        for recv_end in self.recv_ends.iter_mut() {
            let ad = &recv_end.id;
            let key = &recv_end.key;

            match open_in_place(key, &nonce, ad, 0, &mut BytesMut::from(encrypted.clone())) {
                Ok(serialized_plain) => {
                    if recv_end.wnd.try_accept(&nonce) {
                        return Plain::decode(serialized_plain).map_err(|_| ());
                    }
                }
                Err(_) => continue,
            }
        }

        Err(())
    }
}

fn derive_send_recv_end(
    addr: SocketAddr,
    info: NewChannelInfo,
    config: &ChannelConfig
) -> (SendEnd, RecvEnd) {
    let send_end = SendEnd {
        addr,
        id: info.send_end_id,
        nonce: Nonce::zero(),
        key: info.send_end_key,
        keepalive_ticks: config.keepalive_ticks,
    };

    let recv_end = RecvEnd {
        id: info.recv_end_id,
        wnd: NonceWindow::new(config.recv_wnd_size),
        key: info.recv_end_key,
        keepalive_ticks: config.keepalive_ticks,
    };

    (send_end, recv_end)
}

#[cfg(test)]
mod tests {
    use super::*;
    use proto::channeler_udp::PlainContent;

    use ring::aead::CHACHA20_POLY1305;

    #[test]
    fn send_recv() {
        let new_channel_info_a = NewChannelInfo {
            remote_public_key: PublicKey::from_bytes(&vec![1u8; 32]).unwrap(),
            send_end_id: ChannelId::try_from(&[0x00; CHANNEL_ID_LEN][..]).unwrap(),
            send_end_key: SealingKey::new(&CHACHA20_POLY1305, &[0x00; 32][..]).unwrap(),
            recv_end_id: ChannelId::try_from(&[0x01; CHANNEL_ID_LEN][..]).unwrap(),
            recv_end_key: OpeningKey::new(&CHACHA20_POLY1305, &[0x01; 32][..]).unwrap(),
        };

        let new_channel_info_b = NewChannelInfo {
            remote_public_key: PublicKey::from_bytes(&vec![0u8; 32]).unwrap(),
            send_end_id: ChannelId::try_from(&[0x01; CHANNEL_ID_LEN][..]).unwrap(),
            send_end_key: SealingKey::new(&CHACHA20_POLY1305, &[0x01; 32][..]).unwrap(),
            recv_end_id: ChannelId::try_from(&[0x00; CHANNEL_ID_LEN][..]).unwrap(),
            recv_end_key: OpeningKey::new(&CHACHA20_POLY1305, &[0x00; 32][..]).unwrap(),
        };

        let channel_config = ChannelConfig {
            max_recv_end: 3,
            recv_wnd_size: 256,
            keepalive_ticks: 100,
        };

        let addr_a = "127.0.0.1:10001".parse().unwrap();
        let addr_b = "127.0.0.1:10002".parse().unwrap();

        let mut channel_a = Channel::new(addr_a, new_channel_info_a, channel_config);
        let mut channel_b = Channel::new(addr_b, new_channel_info_b, channel_config);

        let plain_to_b = Plain {
            rand_padding: Bytes::from(vec![0xff; 32]),
            content: PlainContent::User(Bytes::from("hello!")),
        };

        let (_addr, msg) = channel_a.pre_send(plain_to_b.clone()).unwrap();
        match msg {
            ChannelerMessage::Encrypted(encrypted) => {
                let content_from_b = channel_b.try_recv(encrypted).unwrap();
                assert_eq!(Some(Bytes::from("hello!")), content_from_b);
            }
            _ => panic!("message type not match"),
        }
    }
}
