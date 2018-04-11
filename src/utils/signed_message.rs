use bytes::{Bytes, BytesMut};
use crypto::identity::Signature;
use crypto::identity;
use crypto::identity::Identity;
use crypto::identity::PublicKey;


/// A signed message.
pub trait SignedMessage {
    /// The signature.
    fn signature(&self) -> &Signature;

    fn set_signature(&mut self, signature: Signature);

    /// The message content, excluding the signature.
    fn as_bytes(&self) -> Bytes;

    fn data_to_sign(&self, extra_data: &[u8]) -> Vec<u8>{
        let mut res = Vec::new();
        res.extend_from_slice(&self.as_bytes());
        res.extend_from_slice(extra_data);
        res
    }

    /// Check whether the signature is valid.
    fn verify_signature(&self, public_key: &PublicKey, extra_data: &[u8]) -> bool{
        identity::verify_signature(&self.data_to_sign(extra_data), public_key, self.signature())
    }

    fn sign(&mut self, extra_data: &[u8], identity: &Identity){
        self.set_signature(identity.sign_message(&self.data_to_sign(extra_data)));
    }
}


pub fn ref_to_bytes(data: &[u8]) -> Bytes {
    let mut buffer = BytesMut::with_capacity(data.len());
    buffer.extend_from_slice(&data);
    buffer.freeze()
}
