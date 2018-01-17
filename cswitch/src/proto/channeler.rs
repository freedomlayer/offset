use bytes::Bytes;

use crypto::rand_values::RandValue;
use crypto::dh::{DhPublicKey, Salt};
use crypto::identity::{PublicKey, Signature};

pub use super::schema_impl::channeler::MessageType;
pub use super::schema_impl::channeler::{serialize_message, deserialize_message};

/// The message intend to be sent by the active end.
#[derive(PartialEq)]
pub struct InitChannelActive {
    /// The identity public key of the sender of this message.
    pub neighbor_public_key: PublicKey,
    /// An initial random value.
    pub channel_rand_value: RandValue,
    /// The index of this channel.
    pub channel_index: u32,
}

/// The message intend to be sent by the passive end.
#[derive(PartialEq)]
pub struct InitChannelPassive {
    /// The identity public key of the sender of this message.
    pub neighbor_public_key: PublicKey,
    /// An initial random value.
    pub channel_rand_value: RandValue,
}

/// The message used in key exchange.
#[derive(PartialEq)]
pub struct Exchange {
    /// Communication public key.
    pub comm_public_key: DhPublicKey,
    /// A salt for the generation of a shared symmetric encryption key.
    pub key_salt: Salt,
    /// Signature over `(channelRandValue || commPublicKey || keySalt)`
    pub signature: Signature,
}

#[derive(PartialEq)]
pub struct EncryptMessage {
    pub inc_counter: u64,
    pub rand_padding: Bytes,
    pub message_type: MessageType,
    pub content: Bytes,
}
