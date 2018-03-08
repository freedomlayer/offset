use crypto::identity::PublicKey;
use proto::funder::InvoiceId;
use proto::common::SendFundsReceipt;
use super::balance_state_old::ProcessMessageError;

#[derive(Clone)]
pub struct InvoiceValidator{
    pub local_invoice_id: Option<InvoiceId>,
    pub remote_invoice_id: Option<InvoiceId>,
}


impl InvoiceValidator{

    pub fn set_remote_invoice_id(&mut self, invoice_id: InvoiceId) -> bool {
        // TODO(a4vision): Should it be self.local_invoice_id here ?
        self.remote_invoice_id = match &self.remote_invoice_id {
            &None => Some(invoice_id),
            &Some(_) => return false,
        };
        true
    }

    pub fn validate_reciept(&mut self, send_funds_receipt: &SendFundsReceipt,
                            public_key: &PublicKey) ->
    Result<(), ProcessMessageError> {
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
