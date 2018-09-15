use std::rc::Rc;
use futures::prelude::{async, await};
use ring::rand::SecureRandom;

use crypto::rand_values::RandValue;
use crypto::identity::{PublicKey, Signature, verify_signature};
use crypto::dh::{DhPublicKey, DhPrivateKey, Salt};
use crypto::sym_encrypt::SymmetricKey;
use identity::client::IdentityClient;
use self::messages::{ExchangeRandNonce, ExchangeDh, ChannelMessage,
                    EncryptedData, PlainData};

mod messages;


pub enum DhError {
    PrivateKeyGenFailure,
    SaltGenFailure,
    DhPublicKeyComputeFailure,
    IncorrectRandNonce,
    InvalidSignature,
}

#[allow(unused)]
pub struct DhStateInitial {
    local_public_key: PublicKey,
    local_rand_nonce: RandValue,
}

#[allow(unused)]
pub struct DhStateHalf {
    remote_public_key: PublicKey,
    local_public_key: PublicKey,
    local_rand_nonce: RandValue,
    dh_private_key: DhPrivateKey,
    local_salt: Salt,
}

#[allow(unused)]
struct Receiver {
    incoming_key: SymmetricKey,
    incoming_counter: u128,
}

#[allow(unused)]
struct Sender {
    outgoing_key: SymmetricKey,
    outgoing_counter: u128,
}

#[allow(unused)]
struct PendingRekey {
    local_dh_public_key: DhPublicKey,
    local_salt: Salt,
}

#[allow(unused)]
pub struct DhState {
    remote_public_key: PublicKey,
    sender: Sender,
    receiver: Receiver,
    /// We might have an old receiver from the last rekeying.
    /// We will remove it upon receipt of the first successful incoming 
    /// messages for the new receiver.
    old_receiver: Option<Receiver>,
    pending_rekey: Option<PendingRekey>,
}



#[allow(unused)]
impl DhStateInitial {
    fn new<R: SecureRandom>(local_public_key: &PublicKey, rng:Rc<R>) -> DhStateInitial {
        DhStateInitial {
            local_public_key: local_public_key.clone(),
            local_rand_nonce: RandValue::new(&*rng),
        }
    }

    #[async]
    fn handle_exchange_rand_nonce<R: SecureRandom + 'static>(self, 
                                                             exchange_rand_nonce: ExchangeRandNonce, 
                                                             identity_client: IdentityClient, rng:Rc<R>) 
                                                            -> Result<(DhStateHalf, ExchangeDh),DhError> {

        let dh_private_key = DhPrivateKey::new(&*rng)
            .map_err(|_| DhError::PrivateKeyGenFailure)?;
        let dh_public_key = dh_private_key.compute_public_key()
                .map_err(|_| DhError::DhPublicKeyComputeFailure)?;;
        let local_salt = Salt::new(&*rng)
            .map_err(|_| DhError::SaltGenFailure)?;

        let dh_state_half = DhStateHalf {
            remote_public_key: exchange_rand_nonce.public_key,
            // remote_rand_nonce: exchange_rand_nonce.rand_nonce,
            local_public_key: self.local_public_key,
            local_rand_nonce: self.local_rand_nonce,
            dh_private_key,
            local_salt: local_salt.clone(),
        };

        let mut exchange_dh = ExchangeDh {
            dh_public_key,
            rand_nonce: exchange_rand_nonce.rand_nonce,
            key_salt: local_salt,
            signature: Signature::zero(),
        };
        exchange_dh.signature = await!(identity_client.request_signature(exchange_dh.signature_buffer()))
            .unwrap();

        Ok((dh_state_half, exchange_dh))
    }
}

#[allow(unused)]
impl DhStateHalf {
    /// Verify the signature at ExchangeDh message
    pub fn verify_exchange_dh(&self, exchange_dh: &ExchangeDh) -> Result<(), DhError> {
        // Verify rand_nonce:
        if self.local_rand_nonce != exchange_dh.rand_nonce {
            return Err(DhError::IncorrectRandNonce);
        }
        // Verify signature:
        let sbuffer = exchange_dh.signature_buffer();
        if !verify_signature(&sbuffer, &self.remote_public_key, &exchange_dh.signature) {
            return Err(DhError::InvalidSignature);
        }
        Ok(())
    }

    fn handle_exchange_dh(self, exchange_dh: ExchangeDh) -> Result<DhState, DhError> {
        self.verify_exchange_dh(&exchange_dh)?;
        // - Combine DhPublicKeys and obtain symmetric key.
        unimplemented!();
    }
}

#[allow(unused)]
pub enum HandleIncomingOutput {
    /// Nothing to do:
    Empty,
    /// This message should be sent to the remote side:
    SendMessage(EncryptedData),
    /// Received an incoming user message
    IncomingUserMessage(PlainData),
}

#[allow(unused)]
impl DhState {
    /// Create an outgoing encrypted message
    pub fn create_outgoing(&mut self, content: PlainData) -> EncryptedData {
        unimplemented!();
    }

    /// Initiate rekeying. Outputs an encrypted message to send to remote side.
    pub fn rekey(&mut self) -> EncryptedData {
        // TODO; How to deal with simultaneous rekeying?
        unimplemented!();
    }

    /// Decrypt an incoming message
    fn decrypt_incoming(&mut self, enc_data: EncryptedData) -> ChannelMessage {
        unimplemented!();
    }

    /// Handle an incoming encrypted message
    pub fn handle_incoming(&mut self, enc_data: EncryptedData) -> HandleIncomingOutput {
        let channel_message = self.decrypt_incoming(enc_data);
        match channel_message {
            ChannelMessage::KeepAlive => 
                HandleIncomingOutput::Empty,
            ChannelMessage::Rekey(rekey) => unimplemented!(),
            ChannelMessage::User(content) => 
                HandleIncomingOutput::IncomingUserMessage(content),
        }
    }
}

