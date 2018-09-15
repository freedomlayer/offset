use std::rc::Rc;
use futures::prelude::{async, await};
use ring::rand::SecureRandom;

use crypto::rand_values::RandValue;
use crypto::identity::{PublicKey, Signature, verify_signature};
use crypto::dh::{DhPublicKey, DhPrivateKey, Salt};
use crypto::sym_encrypt::{Encryptor, Decryptor};
use identity::client::IdentityClient;
use self::messages::{ExchangeRandNonce, ExchangeDh, ChannelMessage,
                    EncryptedData, PlainData};

mod messages;
pub mod serialize;


#[derive(Debug)]
pub enum DhError {
    PrivateKeyGenFailure,
    SaltGenFailure,
    DhPublicKeyComputeFailure,
    IncorrectRandNonce,
    InvalidSignature,
    KeyDerivationFailure,
    CreateEncryptorFailure,
    CreateDecryptorFailure,
    EncryptionFailure,
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
struct PendingRekey {
    local_dh_public_key: DhPublicKey,
    local_salt: Salt,
}

#[allow(unused)]
pub struct DhState {
    local_public_key: PublicKey,
    remote_public_key: PublicKey,
    sender: Encryptor,
    receiver: Decryptor,
    /// We might have an old receiver from the last rekeying.
    /// We will remove it upon receipt of the first successful incoming 
    /// messages for the new receiver.
    old_receiver: Option<Decryptor>,
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

        let (send_key, recv_key) = self.dh_private_key.derive_symmetric_key(
            exchange_dh.dh_public_key,
            self.local_salt,
            exchange_dh.key_salt)
            .map_err(|_| DhError::KeyDerivationFailure)?;

        Ok(DhState {
            local_public_key: self.local_public_key,
            remote_public_key: self.remote_public_key,
            sender: Encryptor::new(&send_key)
                .map_err(|_| DhError::CreateEncryptorFailure)?,
            receiver: Decryptor::new(&recv_key)
                .map_err(|_| DhError::CreateDecryptorFailure)?,
            old_receiver: None,
            pending_rekey: None,
        })
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
    pub fn create_outgoing(&mut self, content: PlainData) -> Result<EncryptedData, DhError> {
        let channel_message = ChannelMessage::User(content);

        // TODO: 
        // - Serialize channel message
        // - Encrypt channel message

        unimplemented!();

        /*
        let encrypted = self.sender.encrypt(&content.0)
            .map_err(|_| DhError::EncryptionFailure)?;
        Ok(EncryptedData(encrypted.to_vec()))
        */
    }

    /// Initiate rekeying. Outputs an encrypted message to send to remote side.
    pub fn rekey(&mut self) -> EncryptedData {
        // TODO; How to deal with simultaneous rekeying?
        unimplemented!();
    }

    /// Decrypt an incoming message
    fn decrypt_incoming(&mut self, enc_data: EncryptedData) -> ChannelMessage {
        // TODO:
        // - Decrypt channel message
        // - Deserialize channel message
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

