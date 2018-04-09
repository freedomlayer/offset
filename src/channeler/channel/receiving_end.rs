use std::convert::TryFrom;
use std::collections::VecDeque;

use bytes::{Bytes, BytesMut};
use ring::aead::{open_in_place, OpeningKey};

use utils::NonceWindow;
use proto::{Proto, channeler::{ChannelId, Plain, PlainContent}};

use super::{Nonce, NONCE_LEN, TAG_LEN, CHANNEL_KEEPALIVE_TIMEOUT, NONCE_WINDOW_LEN};

pub struct ReceivingEnd {
    receiver_channel_id: ChannelId,

    recv_window: NonceWindow,
    opening_key: OpeningKey,

    keepalive_timeout: usize,
}

pub struct CarouselReceivingEnd {
    inner: VecDeque<ReceivingEnd>,
    capacity: usize,
}

impl ReceivingEnd {
    pub fn new(receiver_channel_id: ChannelId, opening_key: OpeningKey)
        -> ReceivingEnd
    {
        ReceivingEnd {
            receiver_channel_id,
            recv_window: NonceWindow::new(NONCE_WINDOW_LEN),
            opening_key,
            keepalive_timeout: 2 * CHANNEL_KEEPALIVE_TIMEOUT,
        }
    }

    #[inline]
    pub fn channel_id(&self) -> &ChannelId {
        &self.receiver_channel_id
    }

    fn decrypt(&mut self, mut encrypted: Bytes) -> Result<Option<Bytes>, ()> {
        let nonce = Nonce::try_from(&encrypted.split_to(NONCE_LEN)).unwrap();

        // Try to open the sealed message
        let plain = open_in_place(&self.opening_key, &nonce,
            &self.receiver_channel_id, 0, &mut BytesMut::from(encrypted)
        ).map_err(|e| {
            error!("failed to open sealed message {:?}", e);
        }).and_then(|serialized_plain| {
            Plain::decode(serialized_plain).map_err(|e| {
                error!("failed to decode serialized plain message: {:?}", e);
            })
        })?;

        if !self.recv_window.try_accept(&nonce) {
            return Err(())
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

    pub fn decrypt(&mut self, channel_id: ChannelId, encrypted: Bytes)
        -> Result<Option<Bytes>, ()>
    {
        assert!(encrypted.len() < NONCE_LEN + TAG_LEN, "message too short");

        for receiving_end in self.inner.iter_mut() {
            if receiving_end.receiver_channel_id == channel_id {
                return receiving_end.decrypt(encrypted);
            }
        }

        unreachable!("not contain specified channel")
    }

    #[inline]
    pub fn ids(&self) -> Vec<&ChannelId> {
        self.inner.iter().map(|receiving_end| {
            &receiving_end.receiver_channel_id
        }).collect()
    }
}