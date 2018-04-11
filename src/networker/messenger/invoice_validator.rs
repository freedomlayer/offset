use crypto::identity::PublicKey;
use proto::funder::InvoiceId;
use proto::common::SendFundsReceipt;
use utils::signed_message::SignedMessage;
use super::token_channel::ProcessMessageError;

#[derive(Clone)]
pub struct InvoiceIds {
    /// The invoice id that my neighbor randomized.
    remote_invoice_id: InvoiceIdStore,
    /// The invoice id which I randomized locally.
    local_invoice_id: InvoiceIdStore,
}


impl InvoiceIds {
    pub fn new(local_invoice_id: Option<InvoiceId>, remote_invoice_id: Option<InvoiceId>)
    -> InvoiceIds {
        InvoiceIds {
            remote_invoice_id: InvoiceIdStore{invoice_id: remote_invoice_id},
            local_invoice_id: InvoiceIdStore{invoice_id: local_invoice_id},
        }
    }

    /// Sets the invoice id randomized  by the neighbor.
    /// Returns `true` if the invoice id was successfully set to the given value.
    pub fn set_remote_invoice_id(&mut self, invoice_id: InvoiceId) -> bool {
        self.remote_invoice_id.set_invoice_id(invoice_id)
    }

    /// Validate the signature and the invoice id of the given receipt.
    pub fn validate_receipt(&self, send_funds_receipt: &SendFundsReceipt,
                            public_key: &PublicKey) ->
    Result<(), ProcessMessageError> {
        self.local_invoice_id.validate_receipt(send_funds_receipt, public_key)
    }

    pub fn local_invoice_id(&self) -> &Option<InvoiceId>{
        self.local_invoice_id.invoice_id()
    }

    pub fn remote_invoice_id(&self) -> &Option<InvoiceId>{
        self.remote_invoice_id.invoice_id()
    }

    pub fn reset_local_invoice_id(&mut self){
        self.local_invoice_id.reset();
    }

    /// Sets the invoice id locally randomized.
    /// Returns `true` if the invoice id was successfully set to the given value.
    pub fn set_local_invoice_id(&mut self, invoice_id: InvoiceId) -> bool {
        self.local_invoice_id.set_invoice_id(invoice_id)
    }
}

#[derive(Clone)]
struct InvoiceIdStore {
    invoice_id: Option<InvoiceId>,
}

impl InvoiceIdStore {
    /// Returns `true` if the invoice id was successfully set to the given value.
    fn set_invoice_id(&mut self, new_invoice_id: InvoiceId) -> bool{
        if self.invoice_id.is_none(){
            self.invoice_id = Some(new_invoice_id);
            true
        }else{
            false
        }
    }

    fn invoice_id(&self) -> &Option<InvoiceId> {
        &self.invoice_id
    }

    fn validate_receipt(&self, receipt: &SendFundsReceipt,
                            public_key: &PublicKey) ->
    Result<(), ProcessMessageError> {
        if !receipt.verify_signature(public_key, &[]) {
            return Err(ProcessMessageError::InvalidFundsReceipt);
        }

        match self.invoice_id{
            Some(ref invoice_id) => {
                if invoice_id != &receipt.invoice_id {
                    return Err(ProcessMessageError::InvalidInvoiceId);
                }
            },
            None => return Err(ProcessMessageError::MissingInvoiceId),
        };

        Ok(())
    }

    fn reset(&mut self) {
        self.invoice_id = None;
    }
}


#[cfg(test)]
mod test{
    use super::*;

    use crypto::identity::PUBLIC_KEY_LEN;
    use crypto::identity::PublicKey;
    use crypto::hash::HASH_RESULT_LEN;
    use crypto::hash::HashResult;
    use crypto::identity::SIGNATURE_LEN;
    use crypto::identity::Signature;
    use crypto::rand_values::RAND_VALUE_LEN;
    use crypto::rand_values::RandValue;
    use proto::funder::INVOICE_ID_LEN;
    use proto::funder::InvoiceId;
    use ring;

    use std::convert::TryFrom;
    use crypto::identity::{Identity, SoftwareEd25519Identity};

    #[test]
    fn test_validate_receipt() {
        let fixed_rand = ring::test::rand::FixedByteRandom { byte: 0x1 };
        let pkcs8 = ring::signature::Ed25519KeyPair::generate_pkcs8(&fixed_rand).unwrap();
        let identity = SoftwareEd25519Identity::from_pkcs8(&pkcs8).unwrap();

        let hash = HashResult::try_from(&[0x01u8; HASH_RESULT_LEN][..]).unwrap();
        let payment = 10u128;
        let rand_nonce = RandValue::try_from(&[0x04f; RAND_VALUE_LEN]).unwrap();


        let mut validator0 = InvoiceIds::new(None, None);
        let invoice_id1 = InvoiceId::try_from(&[0x02; INVOICE_ID_LEN]).unwrap();
        let mut validator1 = InvoiceIds::new(Some(invoice_id1), None);
        let invoice_id2 = InvoiceId::try_from(&[0x03; INVOICE_ID_LEN]).unwrap();
        let mut validator2 = InvoiceIds::new(Some(invoice_id2.clone()), None);

        let invalid_signature = Signature::try_from(&[0x05; SIGNATURE_LEN]).unwrap();
        let mut receipt = SendFundsReceipt::new(hash, &invoice_id2,
        payment, rand_nonce, invalid_signature);


        assert_eq!(Err(ProcessMessageError::InvalidFundsReceipt), validator0.validate_receipt(&receipt, &identity.get_public_key()));
        receipt.sign(&[], &identity);
        assert_eq!(Err(ProcessMessageError::MissingInvoiceId), validator0.validate_receipt(&receipt, &identity.get_public_key()));
        assert_eq!(Err(ProcessMessageError::InvalidInvoiceId), validator1.validate_receipt(&receipt, &identity.get_public_key()));
        assert_eq!(Ok(()), validator2.validate_receipt(&receipt, &identity.get_public_key()));
    }

}
