use byteorder::{WriteBytesExt, BigEndian};

use crypto::hash::HashResult;

use crypto::identity::Signature;

use proto::funder::InvoiceId;
use crypto::identity::{verify_signature, PublicKey};
use funder::signature_buff::FUND_SUCCESS_PREFIX;

/// A `SendFundsReceipt` is received if a `RequestSendFunds` is successful.
/// It can be used a proof of payment for a specific `invoice_id`.
#[derive(Clone)]
pub struct SendFundsReceipt {
    pub response_hash: HashResult,
    // = sha512/256(requestId || sha512/256(route) || randNonce)
    pub invoice_id: InvoiceId,
    pub dest_payment: u128,
    pub signature: Signature,
    // Signature{key=recipientKey}(
    //   "FUND_SUCCESS" ||
    //   sha512/256(requestId || sha512/256(route) || randNonce) ||
    //   invoiceId ||
    //   destPayment
    // )
}


impl SendFundsReceipt {
    pub fn verify_signature(&self, public_key: &PublicKey) -> bool {
        let mut data = Vec::new();
        data.extend(FUND_SUCCESS_PREFIX);
        data.extend(self.response_hash.as_ref());
        data.extend(self.invoice_id.as_ref());
        data.write_u128::<BigEndian>(self.dest_payment).unwrap();
        verify_signature(&data, public_key, &self.signature)
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let mut res_bytes = Vec::new();
        res_bytes.extend_from_slice(&self.response_hash);
        res_bytes.extend_from_slice(&self.invoice_id);
        res_bytes.write_u128::<BigEndian>(self.dest_payment).unwrap();
        res_bytes.extend_from_slice(&self.signature);
        res_bytes
    }
}
