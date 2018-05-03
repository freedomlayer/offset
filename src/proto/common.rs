use bytes::Bytes;
use byteorder::{WriteBytesExt, BigEndian};
use utils::signed_message::SignedMessage;
use utils::signed_message;
use crypto::hash::HashResult;

use crypto::identity::Signature;
use crypto::rand_values::RandValue;

use proto::funder::InvoiceId;

// TODO: implement Receipt

/// A `SendFundsReceipt` is received if a `RequestSendFunds` is successful.
/// It can be used as a proof of payment for a specific `invoice_id`.
#[derive(Clone)]
pub struct SendFundsReceipt {
    response_hash: HashResult, // What is the definition of this hash ?
    pub invoice_id: InvoiceId,
    pub payment: u128,
    rand_nonce: RandValue,
    signature: Signature,
}


impl SignedMessage for SendFundsReceipt{
    fn signature(&self) -> &Signature{
        &self.signature
    }

    fn set_signature(&mut self, signature: Signature){
        self.signature = signature
    }

    fn as_bytes(&self) -> Bytes{
        let mut data = Vec::new();
        data.extend(b"FUND_SUCCESS");
        data.extend(self.response_hash.as_ref());
        data.extend(self.invoice_id.as_ref());
        data.write_u128::<BigEndian>(self.payment)
            .expect("Error writing u128 into data");
        data.extend(self.rand_nonce.as_ref());
        signed_message::ref_to_bytes(data.as_ref())
    }
}

impl SendFundsReceipt {
    pub fn new(response_hash: HashResult, invoice_id: &InvoiceId, payment: u128,
               rand_nonce: RandValue, signature: Signature,) -> SendFundsReceipt{
        SendFundsReceipt{response_hash, invoice_id: invoice_id.clone(), payment, rand_nonce, signature}
    }
}
