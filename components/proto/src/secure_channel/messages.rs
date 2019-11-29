use capnp_conv::{capnp_conv, CapnpConvError, ReadCapnp, WriteCapnp};

use crate::crypto::{DhPublicKey, PublicKey, RandValue, Salt, Signature};

#[capnp_conv(crate::dh_capnp::exchange_rand_nonce::opt_dest_public_key)]
#[derive(Debug, PartialEq, Eq)]
enum OptDestPublicKey {
    Empty,
    PublicKey(PublicKey),
}

impl From<Option<PublicKey>> for OptDestPublicKey {
    fn from(from: Option<PublicKey>) -> Self {
        match from {
            Some(public_key) => Self::PublicKey(public_key),
            None => Self::Empty,
        }
    }
}

impl From<OptDestPublicKey> for Option<PublicKey> {
    fn from(from: OptDestPublicKey) -> Self {
        match from {
            OptDestPublicKey::PublicKey(public_key) => Some(public_key),
            OptDestPublicKey::Empty => None,
        }
    }
}

/// First Diffie-Hellman message:
#[capnp_conv(crate::dh_capnp::exchange_rand_nonce)]
#[derive(Debug, PartialEq, Eq)]
pub struct ExchangeRandNonce {
    pub rand_nonce: RandValue,
    pub src_public_key: PublicKey,
    #[capnp_conv(with = OptDestPublicKey)]
    pub opt_dest_public_key: Option<PublicKey>,
}

/// Second Diffie-Hellman message:
#[capnp_conv(crate::dh_capnp::exchange_dh)]
#[derive(Debug, PartialEq, Eq)]
pub struct ExchangeDh {
    pub dh_public_key: DhPublicKey,
    pub rand_nonce: RandValue,
    pub key_salt: Salt,
    pub signature: Signature,
}

impl ExchangeDh {
    pub fn signature_buffer(&self) -> Vec<u8> {
        let mut sbuffer = Vec::new();
        sbuffer.extend_from_slice(&self.dh_public_key);
        sbuffer.extend_from_slice(&self.rand_nonce);
        sbuffer.extend_from_slice(&self.key_salt);
        sbuffer
    }
}

#[capnp_conv(crate::dh_capnp::rekey)]
#[derive(Debug, PartialEq, Eq, Clone)]
pub struct Rekey {
    pub dh_public_key: DhPublicKey,
    pub key_salt: Salt,
}

#[capnp_conv(crate::dh_capnp::channel_content)]
#[derive(Debug, PartialEq, Eq, Clone)]
pub enum ChannelContent {
    Rekey(Rekey),
    User(Vec<u8>),
}

#[capnp_conv(crate::dh_capnp::channel_message)]
#[derive(Debug, PartialEq, Eq)]
pub struct ChannelMessage {
    pub rand_padding: Vec<u8>,
    pub content: ChannelContent,
}
