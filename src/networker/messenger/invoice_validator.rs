use crypto::identity::PublicKey;
use proto::funder::InvoiceId;
use proto::common::SendFundsReceipt;
use super::token_channel::ProcessMessageError;

#[derive(Clone)]
pub struct InvoiceValidator{
    /// The invoice which I randomized locally.
    pub local_invoice_id: Option<InvoiceId>,
    /// The invoice which the neighbor randomized.
    pub remote_invoice_id: Option<InvoiceId>,
}


impl InvoiceValidator{

    /// Sets the invoice id randomized  by the neighbor.
    /// Returns `true` if the invoice id was successfully set to the given value.
    pub fn set_remote_invoice_id(&mut self, invoice_id: InvoiceId) -> bool {
        // TODO(a4vision): Should it be self.local_invoice_id here ?
        self.remote_invoice_id = match &self.remote_invoice_id {
            &None => Some(invoice_id),
            &Some(_) => return false,
        };
        true
    }

    /// Validate the signature and the invoice id of the given receipt.
    pub fn validate_receipt(&mut self, send_funds_receipt: &SendFundsReceipt,
                            public_key: &PublicKey) ->
    Result<(), ProcessMessageError> {
        // TODO(a4vision): Don't create the errors here, but in the Token Channel.
        if !send_funds_receipt.verify(public_key) {
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
