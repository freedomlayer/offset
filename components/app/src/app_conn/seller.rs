use proto::crypto::InvoiceId;

use proto::app_server::messages::AppRequest;
use proto::funder::messages::{AddInvoice, Commit, Currency};

pub fn add_invoice(
    invoice_id: InvoiceId,
    currency: Currency,
    total_dest_payment: u128,
) -> AppRequest {
    let add_invoice = AddInvoice {
        invoice_id,
        currency,
        total_dest_payment,
    };
    AppRequest::AddInvoice(add_invoice)
}

pub fn cancel_invoice(invoice_id: InvoiceId) -> AppRequest {
    AppRequest::CancelInvoice(invoice_id)
}

pub fn commit_invoice(commit: Commit) -> AppRequest {
    AppRequest::CommitInvoice(commit)
}
