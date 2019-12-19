use proto::crypto::{InvoiceId, PaymentId, PublicKey, Uid};

use proto::app_server::messages::AppRequest;
use proto::funder::messages::{
    AckClosePayment, CreatePayment, CreateTransaction, Currency, FriendsRoute,
};

pub fn create_payment(
    payment_id: PaymentId,
    invoice_id: InvoiceId,
    currency: Currency,
    total_dest_payment: u128,
    dest_public_key: PublicKey,
) -> AppRequest {
    let create_payment = CreatePayment {
        payment_id,
        invoice_id,
        currency,
        total_dest_payment,
        dest_public_key,
    };

    AppRequest::CreatePayment(create_payment)
}

pub fn create_transaction(
    payment_id: PaymentId,
    request_id: Uid,
    route: FriendsRoute,
    dest_payment: u128,
    fees: u128,
) -> AppRequest {
    let create_transaction = CreateTransaction {
        payment_id,
        request_id,
        route,
        dest_payment,
        fees,
    };

    AppRequest::CreateTransaction(create_transaction)
}

pub fn request_close_payment(payment_id: PaymentId) -> AppRequest {
    AppRequest::RequestClosePayment(payment_id)
}

pub fn ack_close_payment(payment_id: PaymentId, ack_uid: Uid) -> AppRequest {
    let ack_close_payment = AckClosePayment {
        payment_id,
        ack_uid,
    };

    AppRequest::AckClosePayment(ack_close_payment)
}
