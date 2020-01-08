use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};

use common::mutable_state::MutableState;
use common::never::Never;
use common::ser_utils::{ser_b64, ser_map_b64_any, ser_string};

use app::common::{Commit, Currency, InvoiceId, MultiRoute, PaymentId, PublicKey, Receipt, Uid};

use route::MultiRouteChoice;

#[allow(clippy::large_enum_variant)]
#[derive(Arbitrary, Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OpenInvoice {
    #[serde(with = "ser_string")]
    pub currency: Currency,
    #[serde(with = "ser_string")]
    pub total_dest_payment: u128,
    /// Invoice description
    pub description: String,
}

#[derive(Arbitrary, Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OpenPaymentStatusSending {
    #[serde(with = "ser_string")]
    pub fees: u128,
    pub open_transactions: HashSet<Uid>,
}

#[derive(Arbitrary, Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OpenPaymentStatusFoundRoute {
    #[serde(with = "ser_b64")]
    pub confirm_id: Uid,
    pub multi_route: MultiRoute,
    pub multi_route_choice: MultiRouteChoice,
    #[serde(with = "ser_string")]
    pub fees: u128,
}

#[allow(clippy::large_enum_variant)]
#[derive(Arbitrary, Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum OpenPaymentStatus {
    SearchingRoute(#[serde(with = "ser_b64")] Uid), // request_routes_id
    FoundRoute(OpenPaymentStatusFoundRoute),
    Sending(OpenPaymentStatusSending),
    Commit(Commit, #[serde(with = "ser_string")] u128), // (commit, fees)
    Success(
        Receipt,
        #[serde(with = "ser_string")] u128,
        #[serde(with = "ser_b64")] Uid,
    ), // (Receipt, fees, ack_uid)
    Failure(#[serde(with = "ser_b64")] Uid),            // ack_uid
}

#[derive(Arbitrary, Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OpenPayment {
    #[serde(with = "ser_b64")]
    pub invoice_id: InvoiceId,
    #[serde(with = "ser_string")]
    pub currency: Currency,
    #[serde(with = "ser_b64")]
    pub dest_public_key: PublicKey,
    #[serde(with = "ser_string")]
    pub dest_payment: u128,
    /// Invoice description (Obtained from the corresponding invoice)
    pub description: String,
    /// Current status of open payment
    pub status: OpenPaymentStatus,
}

#[derive(Arbitrary, Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CompactState {
    /// Seller's open invoices:
    #[serde(with = "ser_map_b64_any")]
    pub open_invoices: HashMap<InvoiceId, OpenInvoice>,
    /// Buyer's open payments:
    #[serde(with = "ser_map_b64_any")]
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
