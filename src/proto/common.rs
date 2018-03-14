use byteorder::{WriteBytesExt, BigEndian};

use crypto::hash::HashResult;

use crypto::identity::Signature;
use crypto::rand_values::RandValue;

use proto::funder::InvoiceId;
use crypto::identity::{verify_signature, PublicKey, Identity};

// TODO: impl Receipt

/// A `SendFundsReceipt` is received if a `RequestSendFunds` is successful.
/// It can be used a proof of payment for a specific `invoice_id`.
pub struct SendFundsReceipt {
    response_hash: HashResult,
    pub invoice_id: InvoiceId,
    pub payment: u128,
    rand_nonce: RandValue,
    signature: Signature,
}


impl SendFundsReceipt {

    pub fn new(response_hash: HashResult, invoice_id: &InvoiceId, payment: u128,
               rand_nonce: RandValue, signature: Signature,) -> SendFundsReceipt{
        SendFundsReceipt{response_hash, invoice_id: invoice_id.clone(), payment, rand_nonce, signature}
    }

    pub fn raw_data_to_sign(&self) -> Vec<u8>{
        let mut data = Vec::new();
        data.extend("FUND_SUCCESS".as_bytes());
        data.extend(self.response_hash.as_ref());
        data.extend(self.invoice_id.as_ref());
        data.write_u128::<BigEndian>(self.payment)
            .expect("Error writing u128 into data");
        data.extend(self.rand_nonce.as_ref());
        data
    }

    // TODO(a4vision): Create a trait for Signable messages.
    pub fn verify_signature(&self, public_key: &PublicKey) -> bool {
        let data = self.raw_data_to_sign();
        verify_signature(&data, public_key, &self.signature)
    }

    // TODO(a4vision): Create a trait for Signable messages.
    pub fn sign(&mut self, identity: &Identity) {
        let data = self.raw_data_to_sign();
        self.signature = identity.sign_message(&data);
    }
}
