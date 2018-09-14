use crypto::identity::{Signature, PublicKey};
use crypto::dh::DhPublicKey;
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
    pub key_salt: RandValue, // ? Not long enough ?
    pub signature: Signature,
}

#[allow(unused)]
pub struct Rekey {
    pub dh_public_key: DhPublicKey,
    pub key_salt: RandValue, // ? Not long enough ?
}

#[allow(unused)]
pub enum ChannelMessage {
    KeepAlive,
    Rekey(Rekey),
    User(PlainData),
}

