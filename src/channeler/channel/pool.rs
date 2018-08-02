use std::net::SocketAddr;
use std::convert::TryFrom;
use std::collections::HashMap;

use bytes::Bytes;

use crypto::identity::PublicKey;
use proto::{
    Proto,
    channeler::{ChannelId, CHANNEL_ID_LEN, Plain, ChannelerMessage},
};
use channeler::handshake::ChannelMetadata;

use super::{Tx, Rx, Channel, ChannelError, MINIMUM_MESSAGE_LEN};

pub struct ChannelPool {
    channels: HashMap<PublicKey, Channel>,

    channel_id_to_public_key: HashMap<ChannelId, PublicKey>,
}

impl ChannelPool {
    pub fn insert_channel(&mut self, remote_addr: SocketAddr, metadata: ChannelMetadata) {
        // NOTE: If channel id conflict, ignore this INSERT operation.
        if self.channel_id_to_public_key.contains_key(&metadata.tx_channel_id)
            || self.channel_id_to_public_key.contains_key(&metadata.rx_channel_id) {
            return
        }

        let tx = Tx::new(remote_addr, metadata.tx_channel_id, metadata.tx_sealing_key);
        let rx = Rx::new(metadata.rx_channel_id, metadata.rx_opening_key);

        self.channel_id_to_public_key
            .insert(tx.channel_id().clone(), metadata.remote_public_key.clone());
        self.channel_id_to_public_key
            .insert(rx.channel_id().clone(), metadata.remote_public_key.clone());

        if let Some(old_channel) = self.channels.get_mut(&metadata.remote_public_key) {
            let (old_tx, old_rx) = old_channel.update_channel(tx, rx);

            if let Some(tx) = old_tx {
                self.channel_id_to_public_key.remove(tx.channel_id());
            }
            if let Some(rx) = old_rx {
                self.channel_id_to_public_key.remove(rx.channel_id());
            }
        } else {
            let new_channel = Channel::new(tx, rx);
            self.channels.insert(metadata.remote_public_key.clone(), new_channel);
        }
    }

    pub fn get_public_key(&self, channel_id: &ChannelId) -> Option<&PublicKey> {
        self.channel_id_to_public_key.get(channel_id)
    }

    pub fn remove_channel(&mut self, public_key: &PublicKey) {
        if let Some(channel) = self.channels.remove(public_key) {
            if let Some(tx) = channel.tx {
                self.channel_id_to_public_key.remove(tx.channel_id());
            }
            for rx in channel.carousel_rx {
                self.channel_id_to_public_key.remove(rx.channel_id());
            }
        }
    }

    pub fn remove_channel_tx(&mut self, public_key: &PublicKey) {
        if let Some(channel) = self.channels.get_mut(public_key) {
            if let Some(channel_id) = channel.remove_sender() {
                self.channel_id_to_public_key.remove(&channel_id);
            }
        }
    }

    pub fn is_connected(&self, public_key: &PublicKey) -> bool {
        if let Some(channel) = self.channels.get(public_key) {
            channel.can_send_message()
        } else {
            false
        }
    }

    pub fn encrypt_msg(&mut self, public_key: &PublicKey, plain: Plain) -> Result<(SocketAddr, Bytes), ChannelError> {
        let channel = self.channels.get_mut(public_key).ok_or(ChannelError::Disconnected)?;

        channel.encrypt_msg(plain).and_then(|(remote_addr, encrypted)| {
            ChannelerMessage::Encrypted(encrypted)
                .encode()
                .map_err(ChannelError::ProtoError)
                .and_then(move |message| Ok((remote_addr, message)))
        })
    }

    pub fn decrypt_msg(&mut self, mut encrypted: Bytes) -> Result<(PublicKey, Option<Bytes>), ChannelError> {
        if encrypted.len() <= MINIMUM_MESSAGE_LEN {
            return Err(ChannelError::MessageTooShort);
        }

        let channel_id = ChannelId::try_from(&encrypted.split_to(CHANNEL_ID_LEN)).unwrap();

        match self.channel_id_to_public_key.get(&channel_id) {
            None => Err(ChannelError::UnknownChannel(channel_id)),
            Some(remote_public_key) => {
                self.channels.get_mut(remote_public_key)
                    .expect("channel pool mapping broken, please report this bug!")
                    .decrypt_msg(channel_id, encrypted)
                    .and_then(|msg| Ok((remote_public_key.clone(), msg)))
            }
        }
    }

    pub fn time_tick(&mut self) -> Vec<PublicKey> {
        // FIXME: https://github.com/realcr/cswitch/pull/54#issuecomment-396855438
        let mut keepalive_timeout_fired = Vec::new();

        let borrowed_channel_id_to_public_key = &mut self.channel_id_to_public_key;
        for (remote_public_key, channel) in &mut self.channels {
            for rx in &mut channel.carousel_rx {
                if rx.keepalive_timeout > 0 {
                    rx.keepalive_timeout -= 1;
                }
            }

            // If receiving end expired, remove associated sending end.
            if let Some(latest_rx) = channel.carousel_rx.back() {
                if latest_rx.keepalive_timeout == 0 {
                    if let Some(tx) = channel.tx.take() {
                        borrowed_channel_id_to_public_key.remove(tx.channel_id());
                    }
                }
            }

            // Remove expired receiving ends.
            channel.carousel_rx.retain(|rx| {
                if rx.keepalive_timeout > 0 {
                    true
                } else {
                    // Remove the associated secondary index.
                    borrowed_channel_id_to_public_key.remove(rx.channel_id());
                    false
                }
            });

            if let Some(tx) = channel.tx.as_mut() {
                if tx.keepalive_timeout > 0 {
                    tx.keepalive_timeout -= 1;
                } else {
                    tx.reset_keepalive_timeout();
                    keepalive_timeout_fired.push(remote_public_key.clone());
                }
            }
        }

        // Remove the channel if there is no sending end and receiving end.
        self.channels.retain(|_, channel| !(channel.carousel_rx.is_empty() && channel.tx.is_none()));

        keepalive_timeout_fired
    }
}
