use std::mem;

use byteorder::{BigEndian, ByteOrder};

use derive_more::From;

use crypto::dh::DhPrivateKey;
use crypto::identity::verify_signature;
use crypto::rand::{CryptoRandom, RandGen};
use crypto::sym_encrypt::{Decryptor, Encryptor};

use proto::crypto::{PublicKey, RandValue, Salt, Signature};
use proto::proto_ser::{ProtoDeserialize, ProtoSerialize, ProtoSerializeError};

use identity::IdentityClient;
use proto::secure_channel::messages::{
    ChannelContent, ChannelMessage, ExchangeDh, ExchangeRandNonce, Rekey,
};

use crate::types::{EncryptedData, PlainData};

const MAX_RAND_PADDING: u16 = 0x100;

#[derive(Debug, From)]
pub enum ScStateError {
    UnexpectedRemotePublicKey,
    PrivateKeyGenFailure,
    DhPublicKeyComputeFailure,
    IncorrectRandNonce,
    InvalidSignature,
    KeyDerivationFailure,
    CreateEncryptorFailure,
    CreateDecryptorFailure,
    DecryptionFailure,
    ProtoSerializeError(ProtoSerializeError),
    // DeserializeError,
    RekeyInProgress,
}

pub struct ScStateInitial {
    local_public_key: PublicKey,
    opt_remote_public_key: Option<PublicKey>,
    local_rand_nonce: RandValue,
}

pub struct ScStateHalf {
    pub remote_public_key: PublicKey,
    local_public_key: PublicKey,
    local_rand_nonce: RandValue,
    dh_private_key: DhPrivateKey,
    local_salt: Salt,
}

struct PendingRekey {
    local_dh_private_key: DhPrivateKey,
    local_salt: Salt,
}

pub struct ScState {
    #[allow(unused)]
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

impl ScStateInitial {
    pub fn new<R: CryptoRandom>(
        local_public_key: PublicKey,
        opt_remote_public_key: Option<PublicKey>,
        rng: &R,
    ) -> (ScStateInitial, ExchangeRandNonce) {
        let local_rand_nonce = RandValue::rand_gen(rng);

        let sc_state_initial = ScStateInitial {
            local_public_key: local_public_key.clone(),
            opt_remote_public_key: opt_remote_public_key.clone(),
            local_rand_nonce: local_rand_nonce.clone(),
        };
        let exchange_rand_nonce = ExchangeRandNonce {
            rand_nonce: local_rand_nonce,
            src_public_key: local_public_key,
            opt_dest_public_key: opt_remote_public_key,
        };
        (sc_state_initial, exchange_rand_nonce)
    }

    pub async fn handle_exchange_rand_nonce<R: CryptoRandom + 'static>(
        self,
        exchange_rand_nonce: ExchangeRandNonce,
        identity_client: IdentityClient,
        rng: R,
    ) -> Result<(ScStateHalf, ExchangeDh), ScStateError> {
        // In case we expect a specific remote public key, verify it first:
        if let Some(expected_remote_public_key) = &self.opt_remote_public_key {
            if expected_remote_public_key != &exchange_rand_nonce.src_public_key {
                return Err(ScStateError::UnexpectedRemotePublicKey);
            }
        }

        let dh_private_key =
            DhPrivateKey::new(&rng).map_err(|_| ScStateError::PrivateKeyGenFailure)?;
        let dh_public_key = dh_private_key
            .compute_public_key()
            .map_err(|_| ScStateError::DhPublicKeyComputeFailure)?;
        let local_salt = Salt::rand_gen(&rng);

        let sc_state_half = ScStateHalf {
            remote_public_key: exchange_rand_nonce.src_public_key,
            local_public_key: self.local_public_key,
            local_rand_nonce: self.local_rand_nonce,
            dh_private_key,
            local_salt: local_salt.clone(),
        };

        let mut exchange_dh = ExchangeDh {
            dh_public_key,
            rand_nonce: exchange_rand_nonce.rand_nonce,
            key_salt: local_salt,
            signature: Signature::default(),
        };
        exchange_dh.signature = identity_client
            .request_signature(exchange_dh.signature_buffer())
            .await
            .unwrap();

        Ok((sc_state_half, exchange_dh))
    }
}

impl ScStateHalf {
    /// Verify the signature at ExchangeDh message
    fn verify_exchange_dh(&self, exchange_dh: &ExchangeDh) -> Result<(), ScStateError> {
        // Verify rand_nonce:
        if self.local_rand_nonce != exchange_dh.rand_nonce {
            return Err(ScStateError::IncorrectRandNonce);
        }
        // Verify signature:
        let sbuffer = exchange_dh.signature_buffer();
        if !verify_signature(&sbuffer, &self.remote_public_key, &exchange_dh.signature) {
            return Err(ScStateError::InvalidSignature);
        }
        Ok(())
    }

    pub fn handle_exchange_dh(self, exchange_dh: ExchangeDh) -> Result<ScState, ScStateError> {
        self.verify_exchange_dh(&exchange_dh)?;

        let (send_key, recv_key) = self
            .dh_private_key
            .derive_symmetric_key(
                exchange_dh.dh_public_key,
                self.local_salt,
                exchange_dh.key_salt,
            )
            .map_err(|_| ScStateError::KeyDerivationFailure)?;

        Ok(ScState {
            local_public_key: self.local_public_key,
            remote_public_key: self.remote_public_key,
            sender: Encryptor::new(&send_key).map_err(|_| ScStateError::CreateEncryptorFailure)?,
            receiver: Decryptor::new(&recv_key)
                .map_err(|_| ScStateError::CreateDecryptorFailure)?,
            opt_old_receiver: None,
            opt_pending_rekey: None,
        })
    }
}

pub struct HandleIncomingOutput {
    pub rekey_occurred: bool,
    pub opt_send_message: Option<EncryptedData>,
    pub opt_incoming_message: Option<PlainData>,
}

impl ScState {
    fn encrypt_outgoing<R: CryptoRandom>(
        &mut self,
        channel_content: ChannelContent,
        rng: &R,
    ) -> EncryptedData {
        let channel_message = ChannelMessage {
            rand_padding: self.gen_rand_padding(rng),
            content: channel_content,
        };
        let ser_channel_message = channel_message.proto_serialize();
        let enc_channel_message = self.sender.encrypt(&ser_channel_message).unwrap();
        EncryptedData(enc_channel_message)
    }

    /// First try to decrypt with the old decryptor.
    /// If it doesn't work, try to decrypt with the new decryptor.
    /// If decryption with the new decryptor works, remove the old decryptor.
    fn try_decrypt(&mut self, enc_data: &EncryptedData) -> Result<PlainData, ScStateError> {
        if let Some(ref mut old_receiver) = self.opt_old_receiver {
            if let Ok(data) = old_receiver.decrypt(&enc_data.0) {
                return Ok(PlainData(data));
            }
        };

        let data = self
            .receiver
            .decrypt(&enc_data.0)
            .map_err(|_| ScStateError::DecryptionFailure)?;
        self.opt_old_receiver = None;
        Ok(PlainData(data))
    }

    /// Decrypt an incoming message
    fn decrypt_incoming(
        &mut self,
        enc_data: &EncryptedData,
    ) -> Result<ChannelContent, ScStateError> {
        let data = self.try_decrypt(enc_data)?.0;
        let channel_message = ChannelMessage::proto_deserialize(&data)?;

        Ok(channel_message.content)
    }

    /// Create an outgoing encrypted message
    pub fn create_outgoing<R: CryptoRandom>(
        &mut self,
        plain_data: &PlainData,
        rng: &R,
    ) -> EncryptedData {
        let content = ChannelContent::User(plain_data.0.clone());
        self.encrypt_outgoing(content, rng)
    }

    /// Generate random padding of random variable length
    /// Done to make it harder to collect metadata over lengths of messages
    fn gen_rand_padding<R: CryptoRandom>(&self, rng: &R) -> Vec<u8> {
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
    pub fn create_rekey<R: CryptoRandom>(
        &mut self,
        rng: &R,
    ) -> Result<EncryptedData, ScStateError> {
        if self.opt_pending_rekey.is_some() {
            return Err(ScStateError::RekeyInProgress);
        }
        let dh_private_key = DhPrivateKey::new(rng).unwrap();
        let local_salt = Salt::rand_gen(rng);
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

    fn handle_incoming_rekey<R: CryptoRandom>(
        &mut self,
        rekey: Rekey,
        rng: &R,
    ) -> Result<HandleIncomingOutput, ScStateError> {
        match self.opt_pending_rekey.take() {
            None => {
                let dh_private_key = DhPrivateKey::new(rng).unwrap();
                let local_salt = Salt::rand_gen(rng);
                let dh_public_key = dh_private_key.compute_public_key().unwrap();

                let (send_key, recv_key) = dh_private_key
                    .derive_symmetric_key(rekey.dh_public_key, local_salt.clone(), rekey.key_salt)
                    .map_err(|_| ScStateError::KeyDerivationFailure)?;

                let new_sender =
                    Encryptor::new(&send_key).map_err(|_| ScStateError::CreateEncryptorFailure)?;
                let new_receiver =
                    Decryptor::new(&recv_key).map_err(|_| ScStateError::CreateDecryptorFailure)?;

                self.opt_old_receiver = Some(mem::replace(&mut self.receiver, new_receiver));

                // Create our Rekey message using the old sender:
                let rekey = Rekey {
                    dh_public_key,
                    key_salt: local_salt,
                };
                let rekey_data = self.encrypt_outgoing(ChannelContent::Rekey(rekey), rng);

                self.sender = new_sender;
                Ok(HandleIncomingOutput {
                    rekey_occurred: true,
                    opt_send_message: Some(rekey_data),
                    opt_incoming_message: None,
                })
            }
            Some(pending_rekey) => {
                let (send_key, recv_key) = pending_rekey
                    .local_dh_private_key
                    .derive_symmetric_key(
                        rekey.dh_public_key,
                        pending_rekey.local_salt,
                        rekey.key_salt,
                    )
                    .map_err(|_| ScStateError::KeyDerivationFailure)?;
                self.sender =
                    Encryptor::new(&send_key).map_err(|_| ScStateError::CreateEncryptorFailure)?;
                let new_receiver =
                    Decryptor::new(&recv_key).map_err(|_| ScStateError::CreateDecryptorFailure)?;
                self.opt_old_receiver = Some(mem::replace(&mut self.receiver, new_receiver));
                Ok(HandleIncomingOutput {
                    rekey_occurred: true,
                    opt_send_message: None,
                    opt_incoming_message: None,
                })
            }
        }
    }

    /// Handle an incoming encrypted message
    pub fn handle_incoming<R: CryptoRandom>(
        &mut self,
        enc_data: &EncryptedData,
        rng: &R,
    ) -> Result<HandleIncomingOutput, ScStateError> {
        match self.decrypt_incoming(enc_data)? {
            ChannelContent::Rekey(rekey) => self.handle_incoming_rekey(rekey, rng),
            ChannelContent::User(content) => Ok(HandleIncomingOutput {
                rekey_occurred: false,
                opt_send_message: None,
                opt_incoming_message: Some(PlainData(content)),
            }),
        }
    }

    /// Get the public key of the remote side
    pub fn get_remote_public_key(&self) -> &PublicKey {
        &self.remote_public_key
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::executor::{LocalPool, ThreadPool};
    use futures::task::SpawnExt;
    use futures::{future, FutureExt};

    use proto::crypto::PrivateKey;

    use crypto::identity::{SoftwareEd25519Identity};
    use crypto::test_utils::DummyRandom;
    use crypto::rand::RandGen;

    use identity::create_identity;
    use identity::IdentityClient;

    async fn run_basic_sc_state(
        identity_client1: IdentityClient,
        identity_client2: IdentityClient,
    ) -> Result<(ScState, ScState), ()> {
        let rng1 = DummyRandom::new(&[1u8]);
        let rng2 = DummyRandom::new(&[2u8]);
        let local_public_key1 = identity_client1.request_public_key().await.unwrap();
        let local_public_key2 = identity_client2.request_public_key().await.unwrap();
        let opt_dest_public_key1 = Some(local_public_key2.clone());
        let opt_dest_public_key2 = None;
        let (sc_state_initial1, exchange_rand_nonce1) =
            ScStateInitial::new(local_public_key1.clone(), opt_dest_public_key1, &rng1);
        let (sc_state_initial2, exchange_rand_nonce2) =
            ScStateInitial::new(local_public_key2.clone(), opt_dest_public_key2, &rng2);

        let (sc_state_half1, exchange_dh1) = sc_state_initial1
            .handle_exchange_rand_nonce(
                exchange_rand_nonce2,
                identity_client1.clone(),
                rng1.clone(),
            )
            .await
            .unwrap();
        let (sc_state_half2, exchange_dh2) = sc_state_initial2
            .handle_exchange_rand_nonce(
                exchange_rand_nonce1,
                identity_client2.clone(),
                rng2.clone(),
            )
            .await
            .unwrap();

        let sc_state1 = sc_state_half1.handle_exchange_dh(exchange_dh2).unwrap();
        let sc_state2 = sc_state_half2.handle_exchange_dh(exchange_dh1).unwrap();
        Ok((sc_state1, sc_state2))
    }

    fn send_recv_messages<R: CryptoRandom>(
        sc_state1: &mut ScState,
        sc_state2: &mut ScState,
        rng1: &R,
        rng2: &R,
    ) {
        // Send a few messages 1 -> 2
        for i in 0..5 {
            let plain_data = PlainData(vec![0, 1, 2, 3, 4, i as u8]);
            let enc_data = sc_state1.create_outgoing(&plain_data, rng1);
            let incoming_output = sc_state2.handle_incoming(&enc_data, rng2).unwrap();
            assert_eq!(incoming_output.rekey_occurred, false);
            assert_eq!(incoming_output.opt_send_message, None);
            assert_eq!(incoming_output.opt_incoming_message.unwrap(), plain_data);
        }

        // Send a few messages 2 -> 1:
        for i in 0..5 {
            let plain_data = PlainData(vec![0, 1, 2, 3, 4, i as u8]);
            let enc_data = sc_state2.create_outgoing(&plain_data, rng2);
            let incoming_output = sc_state1.handle_incoming(&enc_data, rng1).unwrap();
            assert_eq!(incoming_output.rekey_occurred, false);
            assert_eq!(incoming_output.opt_send_message, None);
            assert_eq!(incoming_output.opt_incoming_message.unwrap(), plain_data);
        }
    }

    fn rekey_sequential<R: CryptoRandom>(
        sc_state1: &mut ScState,
        sc_state2: &mut ScState,
        rng1: &R,
        rng2: &R,
    ) {
        let rekey_enc_data1 = sc_state1.create_rekey(rng1).unwrap();
        let incoming_output = sc_state2.handle_incoming(&rekey_enc_data1, rng2).unwrap();
        assert_eq!(incoming_output.rekey_occurred, true);
        let rekey_enc_data2 = incoming_output.opt_send_message.unwrap();
        assert_eq!(incoming_output.opt_incoming_message, None);

        let incoming_output = sc_state1.handle_incoming(&rekey_enc_data2, rng1).unwrap();
        assert_eq!(incoming_output.rekey_occurred, true);
        assert_eq!(incoming_output.opt_send_message, None);
        assert_eq!(incoming_output.opt_incoming_message, None);
    }

    fn rekey_simultaneous<R: CryptoRandom>(
        sc_state1: &mut ScState,
        sc_state2: &mut ScState,
        rng1: &R,
        rng2: &R,
    ) {
        let rekey_enc_data1 = sc_state1.create_rekey(rng1).unwrap();
        let rekey_enc_data2 = sc_state2.create_rekey(rng2).unwrap();

        let incoming_output1 = sc_state1.handle_incoming(&rekey_enc_data2, rng1).unwrap();
        let incoming_output2 = sc_state2.handle_incoming(&rekey_enc_data1, rng2).unwrap();

        assert_eq!(incoming_output1.rekey_occurred, true);
        assert_eq!(incoming_output1.opt_send_message, None);
        assert_eq!(incoming_output1.opt_incoming_message, None);

        assert_eq!(incoming_output2.rekey_occurred, true);
        assert_eq!(incoming_output2.opt_send_message, None);
        assert_eq!(incoming_output2.opt_incoming_message, None);
    }

    fn prepare_dh_test() -> (ScState, ScState, DummyRandom, DummyRandom) {
        let rng1 = DummyRandom::new(&[1u8]);
        let private_key = PrivateKey::rand_gen(&rng1);
        let identity1 = SoftwareEd25519Identity::from_private_key(&private_key).unwrap();
        let (requests_sender1, identity_server1) = create_identity(identity1);
        let identity_client1 = IdentityClient::new(requests_sender1);

        let rng2 = DummyRandom::new(&[2u8]);
        let private_key = PrivateKey::rand_gen(&rng2);
        let identity2 = SoftwareEd25519Identity::from_private_key(&private_key).unwrap();
        let (requests_sender2, identity_server2) = create_identity(identity2);
        let identity_client2 = IdentityClient::new(requests_sender2);

        // Start the Identity service:
        let thread_pool = ThreadPool::new().unwrap();
        thread_pool
            .spawn(identity_server1.then(|_| future::ready(())))
            .unwrap();
        thread_pool
            .spawn(identity_server2.then(|_| future::ready(())))
            .unwrap();

        let (sc_state1, sc_state2) = LocalPool::new()
            .run_until(run_basic_sc_state(identity_client1, identity_client2))
            .unwrap();

        (sc_state1, sc_state2, rng1, rng2)
    }

    #[test]
    fn test_basic_sc_state() {
        let (mut sc_state1, mut sc_state2, rng1, rng2) = prepare_dh_test();
        send_recv_messages(&mut sc_state1, &mut sc_state2, &rng1, &rng2);
        rekey_sequential(&mut sc_state1, &mut sc_state2, &rng1, &rng2);
        send_recv_messages(&mut sc_state1, &mut sc_state2, &rng1, &rng2);
        rekey_simultaneous(&mut sc_state1, &mut sc_state2, &rng1, &rng2);
        send_recv_messages(&mut sc_state1, &mut sc_state2, &rng1, &rng2);
    }
    // TODO: Add tests:
    // - Test the usage of old receiver
    // - Test error cases
    //   - deserialize error
    //   - create_rekey() twice
}
