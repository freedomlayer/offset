use byteorder::{WriteBytesExt, BigEndian};

use crypto::hash::HashResult;

use crypto::identity::Signature;
use crypto::rand_values::RandValue;

use proto::funder::InvoiceId;
use crypto::identity::{verify_signature, PublicKey};

/// A `SendFundsReceipt` is received if a `RequestSendFunds` is successful.
/// It can be used a proof of payment for a specific `invoice_id`.
#[derive(Clone)]
pub struct SendFundsReceipt {
    response_hash: HashResult,
    // = sha512/256(requestId ||
    //       sha512/256(nodeIdPath) ||
    //       mediatorPaymentProposal)
    pub invoice_id: InvoiceId,
    pub payment: u128,
    rand_nonce: RandValue,
    signature: Signature,
    // Signature{key=recipientKey}(
    //   "FUND_SUCCESS" ||
    //   sha512/256(requestId || sha512/256(nodeIdPath) || mediatorPaymentProposal) ||
    //   invoiceId ||
    //   payment ||
    //   randNonce)
}


impl SendFundsReceipt {
    pub fn verify_signature(&self, public_key: &PublicKey) -> bool {
        let mut data = Vec::new();
        data.extend(self.response_hash.as_ref());
        data.extend(self.invoice_id.as_ref());
        data.write_u128::<BigEndian>(self.payment)
            .expect("Error writing u128 into data");
        data.extend(self.rand_nonce.as_ref());

        verify_signature(&data, public_key, &self.signature)
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let mut res_bytes = Vec::new();
        res_bytes.extend_from_slice(&self.response_hash);
        res_bytes.extend_from_slice(&self.invoice_id);
        res_bytes.write_u128::<BigEndian>(self.payment)
            .expect("Could not serialize u128!");
        res_bytes.extend_from_slice(&self.rand_nonce);
        res_bytes.extend_from_slice(&self.signature);
        res_bytes
    }
}
