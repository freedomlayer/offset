use std::convert::TryFrom;

use bytes::{Bytes, BytesMut};
use ring::aead::{open_in_place, OpeningKey};

use utils::NonceWindow;
use channeler::config::{CHANNEL_KEEPALIVE_TIMEOUT, NONCE_WINDOW_WIDTH};
use proto::{Proto, channeler::{ChannelId, Plain, PlainContent}};

use super::{ChannelError, Nonce, NONCE_LEN};

pub struct Rx {
    channel_id: ChannelId,
    opening_key: OpeningKey,
    recv_window: NonceWindow,

    pub(super) keepalive_timeout: usize,
}

impl Rx {
    pub fn new(channel_id: ChannelId, opening_key: OpeningKey) -> Rx {
        Rx {
            channel_id,
            opening_key,
            recv_window: NonceWindow::new(NONCE_WINDOW_WIDTH),
            keepalive_timeout: 2 * CHANNEL_KEEPALIVE_TIMEOUT,
        }
    }

    #[inline]
    pub fn channel_id(&self) -> &ChannelId {
        &self.channel_id
    }

    pub fn decrypt(&mut self, mut encrypted: Bytes)
        -> Result<Option<Bytes>, ChannelError>
    {
        let nonce = Nonce::try_from(&encrypted.split_to(NONCE_LEN))
            .map_err(|_| ChannelError::InvalidNonce)?;

        let plain = open_in_place(
            &self.opening_key,
            &nonce,
            &self.channel_id,
            0,
            &mut BytesMut::from(encrypted)
        )
        .map_err(|_e| ChannelError::DecryptFailed)
        .and_then(|encoded_message| {
            Plain::decode(encoded_message).map_err(ChannelError::Proto)
        })?;

        if !self.recv_window.try_accept(&nonce) {
            return Err(ChannelError::InvalidNonce)
        }

        match plain.content {
            PlainContent::Application(app_data) => Ok(Some(app_data)),
            PlainContent::KeepAlive => {
                // Consume keepalive message in place
                self.keepalive_timeout = 2 * CHANNEL_KEEPALIVE_TIMEOUT;
                Ok(None)
            }
        }
    }
}
