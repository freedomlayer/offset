use std::net::SocketAddr;
use std::convert::TryFrom;
use std::collections::HashMap;

use bytes::Bytes;

use crypto::identity::PublicKey;
use proto::{
    Proto,
    channeler::{ChannelId, CHANNEL_ID_LEN, Plain, ChannelerMessage}
};
use channeler::config::CHANNEL_KEEPALIVE_TIMEOUT;
use channeler::handshake::ChannelMetadata;

use super::{Tx, Rx, Channel, Error, MINIMUM_MESSAGE_LEN};

pub struct ChannelPool {
    channels: HashMap<PublicKey, Channel>,
    cid_to_pk: HashMap<ChannelId, PublicKey>,
}

impl ChannelPool {
    pub fn add_channel(&mut self, addr: SocketAddr, meta: ChannelMetadata) {
        let tx = Tx::new(addr, meta.tx_cid, meta.tx_key);
        let rx = Rx::new(meta.rx_cid, meta.rx_key);

        if let Some(cur_channel) = self.channels.get_mut(&meta.remote_public_key) {
            self.cid_to_pk.insert(
                rx.channel_id().clone(),
                meta.remote_public_key
            );
            let expired_rx = cur_channel.update_channel(tx, rx);

            if let Some(rx) = expired_rx {
                self.cid_to_pk.remove(rx.channel_id());
            }
        } else {
            self.cid_to_pk.insert(
                rx.channel_id().clone(),
                meta.remote_public_key.clone()
            );

            let new_channel = Channel::new(tx, rx);
            self.channels.insert(meta.remote_public_key, new_channel);
        }
    }

    pub fn remove_channel(&mut self, pk: &PublicKey) {
        if let Some(channel) = self.channels.remove(pk) {
            for rx in channel.carousel_rx {
                let _ = self.cid_to_pk.remove(rx.channel_id());
            }
        }
    }

    #[inline]
    pub fn is_connected(&self, pk: &PublicKey) -> bool {
        if let Some(channel) = self.channels.get(pk) {
            channel.can_send_msg()
        } else {
            false
        }
    }

    pub fn encrypt_msg(&mut self, pk: &PublicKey, plain: Plain)
        -> Result<(SocketAddr, Bytes), Error>
    {
        let channel = self.channels.get_mut(pk).ok_or(Error::Disconnected)?;

        channel.encrypt_msg(plain).and_then(|(remote_addr, encrypted)| {
            ChannelerMessage::Encrypted(encrypted)
                .encode()
                .map_err(Error::Proto)
                .and_then(move |message| Ok((remote_addr, message)))
        })
    }

    pub fn decrypt_msg(&mut self, mut encrypted: Bytes)
        -> Result<(PublicKey, Option<Bytes>), Error>
    {
        if encrypted.len() <= MINIMUM_MESSAGE_LEN {
            return Err(Error::MessageTooShort);
        }

        let cid = ChannelId::try_from(&encrypted.split_to(CHANNEL_ID_LEN)).unwrap();

        // XXX: Just make borrow checker happy, can
        // we use combinator instead of match expr
        match self.cid_to_pk.get(&cid) {
            None => Err(Error::UnknownChannel(cid)),
            Some(pk) => {
                self.channels.get_mut(pk)
                    .expect("index broken")
                    .decrypt_msg(cid, encrypted)
                    .and_then(|msg| Ok((pk.clone(), msg)))
            }
        }
    }

    pub fn time_tick(&mut self) -> Vec<PublicKey> {
        // FIXME: https://github.com/realcr/cswitch/pull/54#issuecomment-396855438
        let mut keepalive_fired = Vec::new();

        for (public_key, channel) in &mut self.channels {
            for rx in &mut channel.carousel_rx {
                if rx.keepalive_timeout > 0 {
                    rx.keepalive_timeout -= 1;
                } else {
                    self.cid_to_pk.remove(rx.channel_id());
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
                    keepalive_fired.push(public_key.clone());
                    tx.keepalive_timeout = CHANNEL_KEEPALIVE_TIMEOUT;
                }
            }
        }

        self.channels.retain(|_, channel| !channel.carousel_rx.is_empty());

        keepalive_fired
    }
}
