use app::conn::AppConnTuple;

use crate::persist::{CompactState, OpenPaymentStatus};
use crate::types::{ConnPairCompact};

#[allow(unused)]
pub async fn server_init<GI>(_app_conn_tuple: &mut AppConnTuple, 
    _conn_pair_compact: &mut ConnPairCompact,
    compact_state: &CompactState) {

    for (_payment_id, open_payment) in &compact_state.open_payments {
        match &open_payment.status {
            OpenPaymentStatus::SearchingRoute(_request_routes_id) => unimplemented!(),
            OpenPaymentStatus::FoundRoute(_found_route) => unimplemented!(),
            OpenPaymentStatus::Sending(_sending) => unimplemented!(),
            OpenPaymentStatus::Commit(_commit, _fees) => unimplemented!(),
            OpenPaymentStatus::Success(_receipt, _fees, _ack_uid) => unimplemented!(),
            OpenPaymentStatus::Failure(_ack_uid) => unimplemented!(),
        }
    }
    unimplemented!();
}
