use byteorder::{WriteBytesExt, BigEndian};

use crypto::hash::HashResult;

use crypto::identity::Signature;
use crypto::rand_values::RandValue;

use proto::funder::InvoiceId;
use crypto::identity::{verify_signature, PublicKey};

// TODO: impl Receipt

/// A SendFundsReceipt is received if a RequestSendFunds is successful.
/// It can be used a proof of payment for a specific invoice_id.
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
    pub fn verify(&self, public_key: &PublicKey) -> bool {
        let mut data = Vec::new();
        data.extend(self.response_hash.as_ref());
        data.extend(self.invoice_id.as_ref());
        data.write_u128::<BigEndian>(self.payment)
            .expect("Error writing u128 into data");
        data.extend(self.rand_nonce.as_ref());

        verify_signature(&data, public_key, &self.signature)
    }
}
