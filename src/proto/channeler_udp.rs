use bytes::Bytes;
use crypto::rand_values::RandValue;
use crypto::dh::{DhPublicKey, Salt};
use crypto::hash::HashResult;
use crypto::identity::{PublicKey, Signature};

pub const CHANNEL_ID_LEN: usize = 16;

define_ty!(ChannelId, CHANNEL_ID_LEN);

#[derive(Clone, Debug, PartialEq)]
pub struct InitChannel {
    pub rand_nonce: RandValue,
    pub public_key: PublicKey,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ExchangePassive {
    pub prev_hash:     HashResult,
    pub rand_nonce:    RandValue,
    pub public_key:    PublicKey,
    pub dh_public_key: DhPublicKey,
    pub key_salt:      Salt,
    pub signature:     Signature,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ExchangeActive {
    pub prev_hash:     HashResult,
    pub dh_public_key: DhPublicKey,
    pub key_salt:      Salt,
    pub signature:     Signature,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ChannelReady {
    pub prev_hash: HashResult,
    pub signature: Signature,
}

#[derive(Clone, Debug, PartialEq)]
pub struct UnknownChannel {
    pub channel_id: ChannelId,
    pub rand_nonce: RandValue,
    pub signature: Signature,
}

#[derive(Clone, Debug, PartialEq)]
pub enum PlainContent {
    KeepAlive,
    User(Bytes),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Plain {
    pub rand_padding: Bytes,
    pub content: PlainContent,
}

#[derive(Clone, Debug, PartialEq)]
pub enum ChannelerMessage {
    InitChannel(InitChannel),
    ExchangeActive(ExchangeActive),
    ExchangePassive(ExchangePassive),
    ChannelReady(ChannelReady),
    UnknownChannel(UnknownChannel),
    Encrypted(Bytes),
}
