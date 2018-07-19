use std::net::SocketAddr;
use std::collections::VecDeque;

use bytes::Bytes;
use byteorder::{ByteOrder, LittleEndian};

use utils::WindowNonce;
use proto::ProtoError;
use proto::channeler::{Plain, PlainContent, ChannelId, CHANNEL_ID_LEN};
use channeler::config::MAXIMUM_CAROUSEL_RECEIVER;

const TAG_LEN: usize = 16;
const NONCE_LEN: usize = 12;
const MINIMUM_MESSAGE_LEN: usize = CHANNEL_ID_LEN + NONCE_LEN + TAG_LEN;

mod tx;
mod rx;
mod pool;

use self::tx::Tx;
use self::rx::Rx;

pub use self::pool::ChannelPool;

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
    tx: Option<Tx>,
    carousel_rx: VecDeque<Rx>,
}

#[derive(Debug)]
pub enum Error {
    /// Message too short, we discard the message.
    MessageTooShort,

    /// Error may occur in sealing plain message.
    EncryptFailed,

    /// Error may occur in opening sealed message.
    DecryptFailed,

    /// No sending end for a neighbor.
    Disconnected,

    /// When we received a message while there is no channel matches its
    /// `ChannelId`, we report this error, `Channeler` **SHOULD** send a
    /// `UnknownChannel` message to the sender of this message.
    UnknownChannel(ChannelId),

    /// The `Nonce` along with a message doesn't be accepted.
    IllegalNonce,

    /// Error may occur in encoding/decoding message.
    Proto(ProtoError),
}

impl Channel {
    pub fn new(tx: Tx, rx: Rx) -> Channel {
        let mut carousel_rx = VecDeque::with_capacity(MAXIMUM_CAROUSEL_RECEIVER);
        carousel_rx.push_back(rx);

        Channel { tx: Some(tx), carousel_rx }
    }

    fn encrypt_msg(&mut self, msg: Plain) -> Result<(SocketAddr, Bytes), Error> {
        self.tx_mut().and_then(move |tx| {
            tx.encrypt(msg).and_then(|encrypted| {
                Ok((tx.remote_addr(), encrypted))
            })
        })
    }

    fn decrypt_msg(&mut self, cid: ChannelId, msg: Bytes) -> Result<Option<Bytes>, Error> {
        let rx = self.rx_mut(cid)?;

        match rx.decrypt(msg)?.content {
            PlainContent::KeepAlive => {
                rx.reset_keepalive_timeout();
                Ok(None)
            },
            PlainContent::Application(app) => Ok(Some(app))
        }
    }

    #[inline]
    fn tx_mut(&mut self) -> Result<&mut Tx, Error> {
        self.tx.as_mut().ok_or(Error::Disconnected)
    }

    #[inline]
    fn rx_mut(&mut self, cid: ChannelId) -> Result<&mut Rx, Error> {
        for rx in &mut self.carousel_rx {
            if rx.channel_id() == &cid {
                return Ok(rx)
            }
        }

        Err(Error::UnknownChannel(cid))
    }

    #[inline]
    fn can_send_msg(&self) -> bool {
        self.tx.is_some()
    }

    fn remove_tx(&mut self) {
        self.tx = None;
    }

    fn update_channel(&mut self, tx: Tx, rx: Rx) -> Option<Rx> {
        self.tx = Some(tx);
        self.carousel_rx.push_back(rx);

        if self.carousel_rx.len() <= MAXIMUM_CAROUSEL_RECEIVER {
            None
        } else {
            self.carousel_rx.pop_front()
        }
    }
}
