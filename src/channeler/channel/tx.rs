use std::net::SocketAddr;

use bytes::{Bytes, BytesMut};
use ring::aead::{seal_in_place, SealingKey};

use crypto::increase_nonce;
use channeler::config::CHANNEL_KEEPALIVE_TIMEOUT;
use proto::{Proto, channeler::{ChannelId, CHANNEL_ID_LEN, Plain}};

use super::{ChannelError, Nonce, NONCE_LEN, TAG_LEN};

pub struct Tx {
    remote_addr: SocketAddr,
    channel_id: ChannelId,
    sealing_key: SealingKey,
    next_nonce: Nonce,

    pub(super) keepalive_timeout: usize,
}

impl Tx {
    pub fn new(
        remote_addr: SocketAddr,
        channel_id: ChannelId,
        sealing_key: SealingKey
    ) -> Tx {
        Tx {
            remote_addr,
            channel_id,
            sealing_key,
            next_nonce: Nonce::default(),
            keepalive_timeout: CHANNEL_KEEPALIVE_TIMEOUT,
        }
    }

    #[inline]
    pub fn channel_id(&self) -> &ChannelId {
        &self.channel_id
    }

    #[inline]
    pub fn remote_addr(&self) -> SocketAddr {
        self.remote_addr
    }

    pub fn encrypt(&mut self, plain: Plain) -> Result<Bytes, ChannelError> {
        static PREFIX_LEN: usize = CHANNEL_ID_LEN + NONCE_LEN;

        let encoded_message = plain.encode().map_err(ChannelError::Proto)?;

        let buf_len = PREFIX_LEN + encoded_message.len() + TAG_LEN;
        let mut buf = BytesMut::with_capacity(buf_len);

        buf.extend_from_slice(&self.channel_id);
        buf.extend_from_slice(&self.next_nonce);
        buf.extend_from_slice(&encoded_message);
        buf.extend_from_slice(&[0x00; TAG_LEN][..]);

        seal_in_place(
            &self.sealing_key,
            &self.next_nonce,
            &self.channel_id,
            &mut buf[PREFIX_LEN..], TAG_LEN
        )
        .map_err(|_e| ChannelError::EncryptFailed)
        .and_then(|out_len| {
            increase_nonce(&mut self.next_nonce);
            Ok(buf.split_to(PREFIX_LEN + out_len).freeze())
        })
    }
}
