use crypto::identity::{Signature, PublicKey};
use crypto::dh::{DhPublicKey, Salt};
use crypto::rand_values::RandValue;

pub struct EncryptedData(Vec<u8>);
pub struct PlainData(Vec<u8>);

/// First Diffie-Hellman message:
#[allow(unused)]
pub struct ExchangeRandNonce {
    pub rand_nonce: RandValue,
    pub public_key: PublicKey,
}

/// Second Diffie-Hellman message:
#[allow(unused)]
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

#[allow(unused)]
pub struct Rekey {
    pub dh_public_key: DhPublicKey,
    pub key_salt: Salt,
}

#[allow(unused)]
pub enum ChannelMessage {
    KeepAlive,
    Rekey(Rekey),
    User(PlainData),
}

