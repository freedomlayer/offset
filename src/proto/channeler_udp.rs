use bytes::{Bytes, BytesMut, BufMut};
use crypto::rand_values::{RandValue, RAND_VALUE_LEN};
use crypto::dh::{DhPublicKey, DH_PUBLIC_KEY_LEN, Salt, SALT_LEN};
use crypto::hash::{HashResult, HASH_RESULT_LEN};
use crypto::identity::{PublicKey, PUBLIC_KEY_LEN, Signature, SIGNATURE_LEN};

pub const CHANNEL_ID_LEN: usize = 16;

define_fixed_bytes!(ChannelId, CHANNEL_ID_LEN);

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

impl InitChannel {
    #[inline]
    pub fn concat_fields(&self) -> Bytes {
        let mut buffer = BytesMut::with_capacity(RAND_VALUE_LEN + PUBLIC_KEY_LEN);

        buffer.put(self.rand_nonce.as_ref());
        buffer.put(self.public_key.as_ref());

        buffer.freeze()
    }
}

impl ExchangePassive {
    #[inline]
    pub fn concat_fields(&self) -> Bytes {
        let mut buffer = BytesMut::with_capacity(HASH_RESULT_LEN + RAND_VALUE_LEN + PUBLIC_KEY_LEN +
                                                 DH_PUBLIC_KEY_LEN + SALT_LEN);

        buffer.put(self.prev_hash.as_ref());
        buffer.put(self.rand_nonce.as_ref());
        buffer.put(self.public_key.as_ref());
        buffer.put(self.dh_public_key.as_ref());
        buffer.put(self.key_salt.as_ref());

        buffer.freeze()
    }
}


impl ExchangeActive {
    #[inline]
    pub fn concat_fields(&self) -> Bytes {
        let mut buffer = BytesMut::with_capacity(HASH_RESULT_LEN + DH_PUBLIC_KEY_LEN + SALT_LEN);

        buffer.put(self.prev_hash.as_ref());
        buffer.put(self.dh_public_key.as_ref());
        buffer.put(self.key_salt.as_ref());

        buffer.freeze()
    }
}

impl ChannelReady {
    #[inline]
    pub fn concat_fields(&self) -> Bytes {
        Bytes::from(self.prev_hash.as_ref())
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    use std::convert::TryFrom;

    #[test]
    fn test_init_channel_concat_fields() {
        let init_channel = InitChannel {
            rand_nonce: RandValue::try_from(&[0x00; RAND_VALUE_LEN]).unwrap(),
            public_key: PublicKey::from_bytes(&[0x01; PUBLIC_KEY_LEN]).unwrap(),
        };

        let underlying = init_channel.concat_fields();

        assert_eq!(underlying.len(), RAND_VALUE_LEN + PUBLIC_KEY_LEN);
        assert!(&underlying[..RAND_VALUE_LEN].into_iter().all(|x| *x == 0));
        assert!(&underlying[RAND_VALUE_LEN..].into_iter().all(|x| *x == 1));
    }

    #[test]
    fn test_exchange_passive_concat_fields() {
        let exchange_passive = ExchangePassive {
            prev_hash: HashResult::try_from(&[0x01u8; HASH_RESULT_LEN]).unwrap(),
            rand_nonce: RandValue::try_from(&[0x02; RAND_VALUE_LEN]).unwrap(),
            public_key: PublicKey::from_bytes(&[0x03; PUBLIC_KEY_LEN]).unwrap(),
            dh_public_key: DhPublicKey::try_from(&[0x04; DH_PUBLIC_KEY_LEN]).unwrap(),
            key_salt: Salt::try_from(&[0x05; SALT_LEN]).unwrap(),
            signature: Signature::from_bytes(&[0x06; SIGNATURE_LEN]).unwrap(),
        };

        let underlying = exchange_passive.concat_fields();

        let a = HASH_RESULT_LEN;
        let b = a + RAND_VALUE_LEN;
        let c = b + PUBLIC_KEY_LEN;
        let d = c + DH_PUBLIC_KEY_LEN;
        let e = d + SALT_LEN;

        assert_eq!(underlying.len(), e);
        assert!(&underlying[..a].into_iter().all(|x| *x == 1));
        assert!(&underlying[a..b].into_iter().all(|x| *x == 2));
        assert!(&underlying[b..c].into_iter().all(|x| *x == 3));
        assert!(&underlying[c..d].into_iter().all(|x| *x == 4));
        assert!(&underlying[d..e].into_iter().all(|x| *x == 5));
    }


    #[test]
    fn test_exchange_active_concat_fields() {
        let exchange_active = ExchangeActive {
            prev_hash: HashResult::try_from(&[0x01u8; HASH_RESULT_LEN]).unwrap(),
            dh_public_key: DhPublicKey::try_from(&[0x02; DH_PUBLIC_KEY_LEN]).unwrap(),
            key_salt: Salt::try_from(&[0x03; SALT_LEN]).unwrap(),
            signature: Signature::from_bytes(&[0x04; SIGNATURE_LEN]).unwrap(),
        };

        let underlying = exchange_active.concat_fields();

        let a = HASH_RESULT_LEN;
        let b = a + DH_PUBLIC_KEY_LEN;
        let c = b + SALT_LEN;

        assert_eq!(underlying.len(), c);
        assert!(&underlying[..a].into_iter().all(|x| *x == 1));
        assert!(&underlying[a..b].into_iter().all(|x| *x == 2));
        assert!(&underlying[b..c].into_iter().all(|x| *x == 3));
    }


    #[test]
    fn test_channel_ready_concat_fields() {
        let channel_ready = ChannelReady {
            prev_hash: HashResult::try_from(&[0x01u8; HASH_RESULT_LEN]).unwrap(),
            signature: Signature::from_bytes(&[0x02; SIGNATURE_LEN]).unwrap(),
        };

        let underlying = channel_ready.concat_fields();

        assert_eq!(underlying.len(), HASH_RESULT_LEN);
        assert!(underlying.into_iter().all(|x| x == 1));
    }
}
