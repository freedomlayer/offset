use std::mem;
use std::rc::Rc;
use futures::prelude::{async, await};
use ring::rand::SecureRandom;
use byteorder::{BigEndian, ByteOrder};

use crypto::rand_values::RandValue;
use crypto::identity::{PublicKey, Signature, verify_signature};
use crypto::dh::{DhPrivateKey, Salt};
use crypto::sym_encrypt::{Encryptor, Decryptor};
use identity::client::IdentityClient;
use super::messages::{ExchangeRandNonce, ExchangeDh, ChannelContent,
                    EncryptedData, PlainData, ChannelMessage, Rekey};
use super::serialize::{serialize_channel_message, deserialize_channel_message};

const MAX_RAND_PADDING: u16 = 0x100;


#[derive(Debug)]
#[allow(unused)]
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
    DecryptionFailure,
    DeserializeError,
    RekeyInProgress,
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
    local_dh_private_key: DhPrivateKey,
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
    opt_old_receiver: Option<Decryptor>,
    opt_pending_rekey: Option<PendingRekey>,
}



#[allow(unused)]
impl DhStateInitial {
    fn new<R: SecureRandom>(local_public_key: &PublicKey, rng:Rc<R>) -> (DhStateInitial, ExchangeRandNonce) {
        let local_rand_nonce = RandValue::new(&*rng);

        let dh_state_initial = DhStateInitial {
            local_public_key: local_public_key.clone(),
            local_rand_nonce: local_rand_nonce.clone(),
        };
        let exchange_rand_nonce = ExchangeRandNonce {
            rand_nonce: local_rand_nonce,
            public_key: local_public_key.clone(),
        };
        (dh_state_initial, exchange_rand_nonce)
    }

    #[async]
    fn handle_exchange_rand_nonce<R: SecureRandom + 'static>(self, 
                                                             exchange_rand_nonce: ExchangeRandNonce, 
                                                             identity_client: IdentityClient, rng:Rc<R>) 
                                                            -> Result<(DhStateHalf, ExchangeDh), DhError> {

        let dh_private_key = DhPrivateKey::new(&*rng)
            .map_err(|_| DhError::PrivateKeyGenFailure)?;
        let dh_public_key = dh_private_key.compute_public_key()
                .map_err(|_| DhError::DhPublicKeyComputeFailure)?;;
        let local_salt = Salt::new(&*rng)
            .map_err(|_| DhError::SaltGenFailure)?;

        let dh_state_half = DhStateHalf {
            remote_public_key: exchange_rand_nonce.public_key,
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
            opt_old_receiver: None,
            opt_pending_rekey: None,
        })
    }
}

#[allow(unused)]
pub struct HandleIncomingOutput {
    rekey_occured: bool,
    opt_send_message: Option<EncryptedData>,
    opt_incoming_message: Option<PlainData>,
}


#[allow(unused)]
impl DhState {
    fn encrypt_outgoing<R: SecureRandom>(&mut self, channel_content: ChannelContent, rng: &R) -> EncryptedData {
        let channel_message = ChannelMessage {
            rand_padding: self.gen_rand_padding(rng),
            content: channel_content,
        };
        let ser_channel_message = serialize_channel_message(&channel_message);
        let enc_channel_message = self.sender.encrypt(&ser_channel_message).unwrap();
        EncryptedData(enc_channel_message)
    }

    /// First try to decrypt with the old decryptor.
    /// If it doesn't work, try to decrypt with the new decryptor.
    /// If decryption with the new decryptor works, remove the old decryptor.
    fn try_decrypt(&mut self, enc_data: EncryptedData) -> Result<PlainData, DhError> {
        if let Some(ref mut old_receiver) = self.opt_old_receiver {
            if let Ok(data) = old_receiver.decrypt(&enc_data.0) {
                return Ok(PlainData(data));
            }
        };

        let data = self.receiver.decrypt(&enc_data.0)
            .map_err(|_| DhError::DecryptionFailure)?;
        self.opt_old_receiver = None;
        Ok(PlainData(data))
    }


    /// Decrypt an incoming message
    fn decrypt_incoming(&mut self, enc_data: EncryptedData) -> Result<ChannelContent, DhError> {
        let data = self.try_decrypt(enc_data)?.0;
        let channel_message = deserialize_channel_message(&data)
            .map_err(|_| DhError::DeserializeError)?;

        Ok(channel_message.content)
    }

    /// Create an outgoing encrypted message
    pub fn create_outgoing<R: SecureRandom>(&mut self, content: PlainData, rng: &R) -> EncryptedData {
        let content = ChannelContent::User(content);
        self.encrypt_outgoing(content, rng)
    }

    fn gen_rand_padding<R: SecureRandom>(&self, rng: &R) -> Vec<u8> {
        assert_eq!(MAX_RAND_PADDING & 0xff, 0);

        // Randomize the length of the random padding:
        let mut len_bytes = [0x00; 2];
        rng.fill(&mut len_bytes[..]).unwrap();
        let padding_len = BigEndian::read_u16(&len_bytes[..]) as usize;

        // Return padding_len random bytes:
        let mut rand_padding = vec![0x00; padding_len];
        rng.fill(&mut rand_padding[..]).unwrap();

        rand_padding
    }

    /// Initiate rekeying. Outputs an encrypted message to send to remote side.
    pub fn create_rekey<R: SecureRandom>(&mut self, rng: &R) -> Result<EncryptedData, DhError> {
        if self.opt_pending_rekey.is_some() {
            return Err(DhError::RekeyInProgress);
        }
        let dh_private_key = DhPrivateKey::new(rng).unwrap();
        let local_salt = Salt::new(rng).unwrap();
        let dh_public_key = dh_private_key.compute_public_key().unwrap();
        let pending_rekey = PendingRekey {
            local_dh_private_key: dh_private_key,
            local_salt: local_salt.clone(),
        };
        self.opt_pending_rekey = Some(pending_rekey);

        let rekey = Rekey {
            dh_public_key,
            key_salt: local_salt,
        };
        Ok(self.encrypt_outgoing(ChannelContent::Rekey(rekey), rng))
    }

    fn handle_incoming_rekey<R: SecureRandom>(&mut self, rekey: Rekey, rng: &R) 
        -> Result<HandleIncomingOutput, DhError> {

        match self.opt_pending_rekey.take() {
            None => {
                let dh_private_key = DhPrivateKey::new(rng).unwrap();
                let local_salt = Salt::new(rng).unwrap();
                let dh_public_key = dh_private_key.compute_public_key().unwrap();

                let (send_key, recv_key) = dh_private_key.derive_symmetric_key(
                    rekey.dh_public_key,
                    local_salt.clone(),
                    rekey.key_salt)
                    .map_err(|_| DhError::KeyDerivationFailure)?;

                let new_sender = Encryptor::new(&send_key)
                    .map_err(|_| DhError::CreateEncryptorFailure)?;
                let new_receiver = Decryptor::new(&recv_key)
                    .map_err(|_| DhError::CreateDecryptorFailure)?;

                self.opt_old_receiver = Some(mem::replace(&mut self.receiver, new_receiver));

                // Create our Rekey message using the old sender:
                let rekey = Rekey {
                    dh_public_key,
                    key_salt: local_salt
                };
                let rekey_data = self.encrypt_outgoing(ChannelContent::Rekey(rekey), rng);

                self.sender = new_sender;
                Ok(HandleIncomingOutput {
                    rekey_occured: true,
                    opt_send_message: Some(rekey_data),
                    opt_incoming_message: None,
                })
            },
            Some(pending_rekey) => {
                let (send_key, recv_key) = pending_rekey.local_dh_private_key.derive_symmetric_key(
                    rekey.dh_public_key,
                    pending_rekey.local_salt,
                    rekey.key_salt)
                    .map_err(|_| DhError::KeyDerivationFailure)?;
                self.sender = Encryptor::new(&send_key)
                    .map_err(|_| DhError::CreateEncryptorFailure)?;
                let new_receiver = Decryptor::new(&recv_key)
                    .map_err(|_| DhError::CreateDecryptorFailure)?;
                self.opt_old_receiver = Some(mem::replace(&mut self.receiver, new_receiver));
                Ok(HandleIncomingOutput {
                    rekey_occured: true,
                    opt_send_message: None,
                    opt_incoming_message: None,
                })
            },
        }
    }

    /// Handle an incoming encrypted message
    pub fn handle_incoming<R: SecureRandom>(&mut self, enc_data: EncryptedData, rng: &R) 
        -> Result<HandleIncomingOutput, DhError> {

        match self.decrypt_incoming(enc_data)? {
            ChannelContent::KeepAlive => 
                Ok(HandleIncomingOutput { rekey_occured: false, 
                    opt_send_message: None, opt_incoming_message: None }),
            ChannelContent::Rekey(rekey) => self.handle_incoming_rekey(rekey, rng),
            ChannelContent::User(content) => 
                Ok(HandleIncomingOutput { rekey_occured: false, 
                    opt_send_message: None, opt_incoming_message: Some(content) }),
        }
    }
}

