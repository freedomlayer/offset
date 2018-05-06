use crypto::identity::PublicKey;
use proto::funder::InvoiceId;
use proto::common::SendFundsReceipt;
use super::token_channel::ProcessMessageError;

#[derive(Clone)]
pub struct InvoiceValidator{
    /// The invoice id which I randomized locally.
    pub local_invoice_id: Option<InvoiceId>,
    /// The invoice id which the neighbor randomized.
    pub remote_invoice_id: Option<InvoiceId>,
}


impl InvoiceValidator{

    /// Sets the invoice id randomized  by the neighbor.
    /// Returns `true` if the invoice id was successfully set to the given value.
    pub fn set_remote_invoice_id(&mut self, invoice_id: InvoiceId) -> bool {
        if self.remote_invoice_id.is_none(){
            self.remote_invoice_id = Some(invoice_id);
            true
        }else{
            false
        }
    }

    /// Validate the signature and the invoice id of the given receipt.
    pub fn validate_receipt(&mut self, send_funds_receipt: &SendFundsReceipt,
                            public_key: &PublicKey) ->
    Result<(), ProcessMessageError> {
        if !send_funds_receipt.verify_signature(public_key) {
            return Err(ProcessMessageError::InvalidFundsReceipt);
        }

        match self.local_invoice_id{
            Some(ref local_invoice_id) => {
                if local_invoice_id != &send_funds_receipt.invoice_id {
                    return Err(ProcessMessageError::InvalidInvoiceId);
                }
            },
            None => return Err(ProcessMessageError::MissingInvoiceId),
        };

        Ok(())
    }
}
