use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use common::mutable_state::MutableState;
use common::never::Never;

use app::common::{Commit, Currency, InvoiceId, PaymentId, PublicKey, Receipt, Uid};
use app::ser_string::{from_base64, from_string, to_base64, to_string};

use database::AtomicDb;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenInvoice {
    #[serde(serialize_with = "to_string", deserialize_with = "from_string")]
    pub currency: Currency,
    #[serde(serialize_with = "to_string", deserialize_with = "from_string")]
    pub total_dest_payment: u128,
    /// Invoice description
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum OpenPaymentStatus {
    SearchingRoute(Uid), // request_routes_id
    // TODO: Possibly add the found route into FoundRoute state?
    FoundRoute(Uid, u128), // (confirm_id, fees)
    Sending(u128),         // fees
    Commit(Commit, u128),  // (commit, fees)

                           // Done(Receipt, u128),   // (receipt, fees)
                           // Failure,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenPayment {
    #[serde(serialize_with = "to_base64", deserialize_with = "from_base64")]
    pub payment_id: PaymentId,
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
    pub open_payments: HashMap<InvoiceId, OpenPayment>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CompactStateDb {
    state: CompactState,
}

impl MutableState for CompactStateDb {
    // We consider the full state to be a mutation.
    // This is somewhat inefficient:
    type Mutation = CompactState;
    type MutateError = Never;

    fn mutate(&mut self, mutation: &Self::Mutation) -> Result<(), Self::MutateError> {
        // We consider the full state to be a mutation:
        self.state = mutation.clone();
        Ok(())
    }
}
