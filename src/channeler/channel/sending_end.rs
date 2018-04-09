use std::net::SocketAddr;

use bytes::{Bytes, BytesMut};
use ring::aead::{seal_in_place, SealingKey};

use crypto::increase_nonce;
use proto::{Proto, channeler::{ChannelId, CHANNEL_ID_LEN, Plain}};

use super::{Nonce, NONCE_LEN, TAG_LEN, CHANNEL_KEEPALIVE_TIMEOUT};

pub struct SendingEnd {
    sender_channel_id: ChannelId,

    remote_addr: SocketAddr,
    nonce: Nonce,
    sealing_key: SealingKey,

    keepalive_timeout: usize,
}

impl SendingEnd {
    pub fn new(
        addr: SocketAddr,
        sender_channel_id: ChannelId,
        sealing_key: SealingKey
    ) -> SendingEnd {
        SendingEnd {
            remote_addr: addr,
            sender_channel_id,
            sealing_key,
            nonce: Nonce::default(),
            keepalive_timeout: CHANNEL_KEEPALIVE_TIMEOUT,
        }
    }

    #[inline]
    pub fn channel_id(&self) -> &ChannelId {
        &self.sender_channel_id
    }

    #[inline]
    pub fn remote_addr(&self) -> SocketAddr {
        self.remote_addr
    }

    pub fn encrypt(&mut self, plain: Plain) -> Result<Bytes, ()> {
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
}
