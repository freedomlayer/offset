use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};

use common::mutable_state::MutableState;
use common::never::Never;

use app::common::{Commit, Currency, InvoiceId, MultiRoute, PaymentId, PublicKey, Receipt, Uid};
use app::ser_string::{from_base64, from_string, to_base64, to_string};

use database::AtomicDb;
use route::MultiRouteChoice;

use crate::messages::{OpenInvoice, OpenPayment};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompactState {
    /// Seller's open invoices:
    pub open_invoices: HashMap<InvoiceId, OpenInvoice>,
    /// Buyer's open payments:
    pub open_payments: HashMap<PaymentId, OpenPayment>,
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
