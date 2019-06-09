use crypto::rand::RandValue;
use crypto::dh::{DhPublicKey, Salt};
use crypto::identity::{PublicKey, Signature};

#[derive(Debug, PartialEq, Eq, Clone)]
pub struct EncryptedData(pub Vec<u8>);
#[derive(Debug, PartialEq, Eq, Clone)]
pub struct PlainData(pub Vec<u8>);

/// First Diffie-Hellman message:
#[derive(Debug, PartialEq, Eq)]
pub struct ExchangeRandNonce {
    pub rand_nonce: RandValue,
    pub public_key: PublicKey,
}

/// Second Diffie-Hellman message:
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

#[derive(Debug, PartialEq, Eq)]
pub struct Rekey {
    pub dh_public_key: DhPublicKey,
    pub key_salt: Salt,
}

#[derive(Debug, PartialEq, Eq)]
pub enum ChannelContent {
    Rekey(Rekey),
    User(PlainData),
}

#[derive(Debug, PartialEq, Eq)]
pub struct ChannelMessage {
    pub rand_padding: Vec<u8>,
    pub content: ChannelContent,
}
