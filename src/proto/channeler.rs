use bytes::{Bytes, BytesMut, BufMut};
use crypto::rand_values::{RandValue, RAND_VALUE_LEN};
use crypto::dh::{DhPublicKey, DH_PUBLIC_KEY_LEN, Salt, SALT_LEN};
use crypto::hash::{HashResult, HASH_RESULT_LEN};
use crypto::identity::{PublicKey, PUBLIC_KEY_LEN, Signature};

pub const CHANNEL_ID_LEN: usize = 16;

define_fixed_bytes!(ChannelId, CHANNEL_ID_LEN);

#[derive(Clone, Debug, PartialEq)]
pub struct RequestNonce {
    pub request_rand_nonce: RandValue,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ResponseNonce {
    pub request_rand_nonce: RandValue,
    pub response_rand_nonce: RandValue,
    pub responder_rand_nonce: RandValue,
    pub signature: Signature,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ExchangeActive {
    pub responder_rand_nonce: RandValue,
    pub initiator_rand_nonce: RandValue,

    pub initiator_public_key: PublicKey,
    pub responder_public_key: PublicKey,
    pub dh_public_key: DhPublicKey,
    pub key_salt: Salt,
    pub signature: Signature,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ExchangePassive {
    pub prev_hash: HashResult,
    pub dh_public_key: DhPublicKey,
    pub key_salt: Salt,
    pub signature: Signature,
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
    Application(Bytes),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Plain {
    pub rand_padding: Bytes,
    pub content: PlainContent,
}

#[derive(Clone, Debug, PartialEq)]
pub enum ChannelerMessage {
    RequestNonce(RequestNonce),
    ResponseNonce(ResponseNonce),
    ExchangeActive(ExchangeActive),
    ExchangePassive(ExchangePassive),
    ChannelReady(ChannelReady),
    UnknownChannel(UnknownChannel),
    Encrypted(Bytes),
}

impl RequestNonce {
    #[inline]
    pub fn as_bytes(&self) -> Bytes {
        Bytes::from(self.request_rand_nonce.as_ref())
    }
}

impl ResponseNonce {
    #[inline]
    pub fn as_bytes(&self) -> Bytes {
        let mut buf = BytesMut::with_capacity(3 * RAND_VALUE_LEN);

        buf.put(self.request_rand_nonce.as_ref());
        buf.put(self.response_rand_nonce.as_ref());
        buf.put(self.responder_rand_nonce.as_ref());

        buf.freeze()
    }
}

impl ExchangeActive {
    #[inline]
    pub fn as_bytes(&self) -> Bytes {
        let mut buf = BytesMut::with_capacity(
            2 * RAND_VALUE_LEN
                + 2 * PUBLIC_KEY_LEN
                + DH_PUBLIC_KEY_LEN
                + SALT_LEN
        );

        buf.put(self.responder_rand_nonce.as_ref());
        buf.put(self.initiator_rand_nonce.as_ref());
        buf.put(self.initiator_public_key.as_ref());
        buf.put(self.responder_public_key.as_ref());
        buf.put(self.dh_public_key.as_ref());
        buf.put(self.key_salt.as_ref());

        buf.freeze()
    }
}

impl ExchangePassive {
    #[inline]
    pub fn as_bytes(&self) -> Bytes {
        let mut buf = BytesMut::with_capacity(
            HASH_RESULT_LEN
                + DH_PUBLIC_KEY_LEN
                + SALT_LEN
        );

        buf.put(self.prev_hash.as_ref());
        buf.put(self.dh_public_key.as_ref());
        buf.put(self.key_salt.as_ref());

        buf.freeze()
    }
}

impl ChannelReady {
    #[inline]
    pub fn as_bytes(&self) -> Bytes {
        Bytes::from(self.prev_hash.as_ref())
    }
}

impl UnknownChannel {
    #[inline]
    pub fn as_bytes(&self) -> Bytes {
        let mut buf = BytesMut::with_capacity(CHANNEL_ID_LEN + RAND_VALUE_LEN);

        buf.put(self.channel_id.as_ref());
        buf.put(self.rand_nonce.as_ref());

        buf.freeze()
    }
}

impl ChannelerMessage {
    #[inline]
    pub fn is_handshake_message(&self) -> bool {
        match *self {
            ChannelerMessage::RequestNonce(_)
            | ChannelerMessage::ResponseNonce(_)
            | ChannelerMessage::ExchangeActive(_)
            | ChannelerMessage::ExchangePassive(_)
            | ChannelerMessage::ChannelReady(_) => true,
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crypto::identity::SIGNATURE_LEN;

    #[test]
    fn test_request_nonce_as_bytes() {
        let request_nonce = RequestNonce {
            request_rand_nonce: RandValue::from(&[0x00; RAND_VALUE_LEN]),
        };

        let underlying = request_nonce.as_bytes();

        assert!(underlying.into_iter().all(|x| x == 0));
    }

    #[test]
    fn test_respond_nonce_as_bytes() {
        let respond_nonce = ResponseNonce {
            request_rand_nonce: RandValue::from(&[0x00; RAND_VALUE_LEN]),
            response_rand_nonce: RandValue::from(&[0x01; RAND_VALUE_LEN]),
            responder_rand_nonce: RandValue::from(&[0x02; RAND_VALUE_LEN]),
            signature: Signature::from(&[0xff; SIGNATURE_LEN]),
        };

        let underlying = respond_nonce.as_bytes();

        let a = RAND_VALUE_LEN;
        let b = a + RAND_VALUE_LEN;
        let n = b + RAND_VALUE_LEN;

        assert_eq!(underlying.len(), n);

        assert!(&underlying[0..a].into_iter().all(|x| *x == 0));
        assert!(&underlying[a..b].into_iter().all(|x| *x == 1));
        assert!(&underlying[b..n].into_iter().all(|x| *x == 2));
    }

    #[test]
    fn test_exchange_active_as_bytes() {
        let exchange_active = ExchangeActive {
            responder_rand_nonce: RandValue::from(&[0x00; RAND_VALUE_LEN]),
            initiator_rand_nonce: RandValue::from(&[0x01; RAND_VALUE_LEN]),
            initiator_public_key: PublicKey::from(&[0x02; PUBLIC_KEY_LEN]),
            responder_public_key: PublicKey::from(&[0x03; PUBLIC_KEY_LEN]),
            dh_public_key: DhPublicKey::from(&[0x04; DH_PUBLIC_KEY_LEN]),
            key_salt: Salt::from(&[0x05; SALT_LEN]),
            signature: Signature::from(&[0xff; SIGNATURE_LEN]),
        };

        let underlying = exchange_active.as_bytes();

        let a = RAND_VALUE_LEN;
        let b = a + RAND_VALUE_LEN;
        let c = b + PUBLIC_KEY_LEN;
        let d = c + PUBLIC_KEY_LEN;
        let e = d + DH_PUBLIC_KEY_LEN;
        let n = e + SALT_LEN;

        assert_eq!(underlying.len(), n);

        assert!(&underlying[0..a].into_iter().all(|x| *x == 0));
        assert!(&underlying[a..b].into_iter().all(|x| *x == 1));
        assert!(&underlying[b..c].into_iter().all(|x| *x == 2));
        assert!(&underlying[c..d].into_iter().all(|x| *x == 3));
        assert!(&underlying[d..e].into_iter().all(|x| *x == 4));
        assert!(&underlying[e..n].into_iter().all(|x| *x == 5));
    }

    #[test]
    fn test_exchange_passive_as_bytes() {
        let exchange_passive = ExchangePassive {
            prev_hash: HashResult::from(&[0x00; HASH_RESULT_LEN]),
            dh_public_key: DhPublicKey::from(&[0x01; DH_PUBLIC_KEY_LEN]),
            key_salt: Salt::from(&[0x02; SALT_LEN]),
            signature: Signature::from(&[0xff; SIGNATURE_LEN]),
        };

        let underlying = exchange_passive.as_bytes();

        let a = HASH_RESULT_LEN;
        let b = a + DH_PUBLIC_KEY_LEN;
        let n = b + SALT_LEN;

        assert_eq!(underlying.len(), n);

        assert!(&underlying[0..a].into_iter().all(|x| *x == 0));
        assert!(&underlying[a..b].into_iter().all(|x| *x == 1));
        assert!(&underlying[b..n].into_iter().all(|x| *x == 2));
    }

    #[test]
    fn test_channel_ready_as_bytes() {
        let channel_ready = ChannelReady {
            prev_hash: HashResult::from(&[0x00; HASH_RESULT_LEN]),
            signature: Signature::from(&[0xff; SIGNATURE_LEN]),
        };

        let underlying = channel_ready.as_bytes();

        assert_eq!(underlying.len(), HASH_RESULT_LEN);
        assert!(underlying.into_iter().all(|x| x == 0));
    }

    #[test]
    fn test_unknown_channel_as_bytes() {
        let unknown_channel = UnknownChannel {
            channel_id: ChannelId::from(&[0x00; CHANNEL_ID_LEN]),
            rand_nonce: RandValue::from(&[0x01; RAND_VALUE_LEN]),
            signature: Signature::from(&[0xff; SIGNATURE_LEN]),
        };

        let underlying = unknown_channel.as_bytes();

        assert_eq!(underlying.len(), CHANNEL_ID_LEN + RAND_VALUE_LEN);

        assert!(&underlying[0..CHANNEL_ID_LEN].into_iter().all(|x| *x == 0));
        assert!(&underlying[RAND_VALUE_LEN..].into_iter().all(|x| *x == 1));
    }
}
