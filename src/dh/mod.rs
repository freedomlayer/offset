use crypto::rand_values::RandValue;
use crypto::identity::PublicKey;
use crypto::sym_encrypt::SymmetricKey;
use self::messages::{ExchangeRandNonce, ExchangeDh, ChannelMessage,
                    EncryptedData, PlainData};

mod messages;

#[allow(unused)]
struct DhStateInitial;

#[allow(unused)]
struct DhStateHalf {
    remote_rand_nonce: RandValue,
    remote_public_key: PublicKey,
}

#[allow(unused)]
struct DhState {
    remote_public_key: PublicKey,
    incoming_key: SymmetricKey,
    outgoing_key: SymmetricKey,
    incoming_counter: u128,
    outgoing_counter: u128,
}



#[allow(unused)]
impl DhStateInitial {
    fn new() -> DhStateInitial {
        DhStateInitial
    }

    fn handle_exchange_rand_nonce(self, exchange_rand_nonce: ExchangeRandNonce) -> DhStateHalf {
        DhStateHalf {
            remote_rand_nonce: exchange_rand_nonce.rand_nonce,
            remote_public_key: exchange_rand_nonce.public_key,
        }
    }
}

#[allow(unused)]
impl DhStateHalf {
    fn handle_exchange_dh(self, exchange_dh: ExchangeDh) -> DhState {
        unimplemented!();
    }
}

#[allow(unused)]
enum HandleIncomingOutput {
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


