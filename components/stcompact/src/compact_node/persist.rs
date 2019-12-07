use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};

use common::mutable_state::MutableState;
use common::never::Never;

use app::common::{Commit, Currency, InvoiceId, MultiRoute, PaymentId, PublicKey, Receipt, Uid};
use app::ser_string::{from_base64, from_string, to_base64, to_string};

use route::MultiRouteChoice;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OpenInvoice {
    #[serde(serialize_with = "to_string", deserialize_with = "from_string")]
    pub currency: Currency,
    #[serde(serialize_with = "to_string", deserialize_with = "from_string")]
    pub total_dest_payment: u128,
    /// Invoice description
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenPaymentStatusSending {
    pub fees: u128,
    pub open_transactions: HashSet<Uid>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenPaymentStatusFoundRoute {
    pub confirm_id: Uid,
    pub multi_route: MultiRoute,
    pub multi_route_choice: MultiRouteChoice,
    pub fees: u128,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum OpenPaymentStatus {
    SearchingRoute(Uid), // request_routes_id
    FoundRoute(OpenPaymentStatusFoundRoute),
    Sending(OpenPaymentStatusSending),
    Commit(Commit, u128),        // (commit, fees)
    Success(Receipt, u128, Uid), // (Receipt, fees, ack_uid)
    Failure(Uid),                // ack_uid
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenPayment {
    #[serde(serialize_with = "to_base64", deserialize_with = "from_base64")]
    pub invoice_id: InvoiceId,
    #[serde(serialize_with = "to_string", deserialize_with = "from_string")]
    pub currency: Currency,
    #[serde(serialize_with = "to_base64", deserialize_with = "from_base64")]
    pub dest_public_key: PublicKey,
    #[serde(serialize_with = "to_string", deserialize_with = "from_string")]
    pub dest_payment: u128,
    /// Invoice description (Obtained from the corresponding invoice)
    pub description: String,
    /// Current status of open payment
    pub status: OpenPaymentStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompactState {
    /// Seller's open invoices:
    pub open_invoices: HashMap<InvoiceId, OpenInvoice>,
    /// Buyer's open payments:
    pub open_payments: HashMap<PaymentId, OpenPayment>,
}

impl CompactState {
    pub fn new() -> Self {
        Self {
            open_invoices: HashMap::new(),
            open_payments: HashMap::new(),
        }
    }
}

impl MutableState for CompactState {
    // We consider the full state to be a mutation.
    // This is somewhat inefficient:
    type Mutation = CompactState;
    type MutateError = Never;

    fn mutate(&mut self, mutation: &Self::Mutation) -> Result<(), Self::MutateError> {
        // We consider the full state to be a mutation:
        *self = mutation.clone();
        Ok(())
    }
}
