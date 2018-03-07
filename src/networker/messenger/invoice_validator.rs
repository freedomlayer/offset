use proto::funder::InvoiceId;

#[derive(Clone)]
pub struct InvoiceValidator{
    pub local_invoice_id: Option<InvoiceId>,
    pub remote_invoice_id: Option<InvoiceId>,
}


impl InvoiceValidator{
    pub fn set_remote_invoice_id(&mut self, invoice_id: InvoiceId) -> bool {
        self.remote_invoice_id = match &self.remote_invoice_id {
            &None => Some(invoice_id),
            &Some(_) => return false,
        };
        true
    }
}
