use std::convert::TryFrom;

use bytes::{Bytes, BytesMut};
use ring::aead::{open_in_place, OpeningKey};

use utils::NonceWindow;
use channeler::config::{CHANNEL_KEEPALIVE_TIMEOUT, NONCE_WINDOW_WIDTH};
use proto::{Proto, channeler::{ChannelId, Plain}};

use super::{Error, Nonce, NONCE_LEN};

pub struct Rx {
    channel_id: ChannelId,
    opening_key: OpeningKey,
    recv_window: NonceWindow,

    pub(super) keepalive_timeout: usize,
}

impl Rx {
    pub fn new(rx_cid: ChannelId, rx_key: OpeningKey) -> Rx {
        Rx {
            channel_id: rx_cid,
            opening_key: rx_key,
            recv_window: NonceWindow::new(NONCE_WINDOW_WIDTH),
            keepalive_timeout: 2 * CHANNEL_KEEPALIVE_TIMEOUT,
        }
    }

    #[inline]
    pub fn channel_id(&self) -> &ChannelId {
        &self.channel_id
    }

    #[inline]
    pub fn reset_keepalive_timeout(&mut self) {
        self.keepalive_timeout = 2 * CHANNEL_KEEPALIVE_TIMEOUT;
    }

    pub fn decrypt(&mut self, mut encrypted: Bytes) -> Result<Plain, Error> {
        let nonce = Nonce::try_from(&encrypted.split_to(NONCE_LEN))
            .expect("message too short");

        let plain = open_in_place(
            &self.opening_key,
            &nonce,
            &self.channel_id,
            0,
            &mut BytesMut::from(encrypted)
        )
        .map_err(|_e| Error::DecryptFailed)
        .and_then(|encoded_message| {
            Plain::decode(encoded_message).map_err(Error::Proto)
        })?;

        if self.recv_window.try_accept(&nonce) {
            Ok(plain)
        } else {
            Err(Error::IllegalNonce)
        }
    }
}
