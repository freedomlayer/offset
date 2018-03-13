//use std::cmp;
//use proto::funder::InvoiceId;
//
///// Maximum value possible for max_debt.
//const MAX_NEIGHBOR_DEBT: u64 = (1 << 63) - 1;
//
//#[derive(Clone)]
//pub struct CreditState {
//    pub remote_max_debt: u64,
//    pub local_max_debt: u64,
//    pub remote_pending_debt: u64,
//    pub local_pending_debt: u64,
//    pub balance: i64,
//    pub local_invoice_id: Option<InvoiceId>,
//    pub remote_invoice_id: Option<InvoiceId>,
//}
//
//impl CreditState {
//    pub fn set_local_max_debt(&mut self, proposed_max_debt: u64) -> bool {
//        let max_local_max_debt: u64 = cmp::min(
//            MAX_NEIGHBOR_DEBT as i64,
//            (self.local_pending_debt as i64) - self.balance) as u64;
//
//        if proposed_max_debt > max_local_max_debt {
//            false
//        } else {
//            self.local_max_debt = proposed_max_debt;
//            true
//        }
//    }
//
//    pub fn set_remote_invoice_id(&mut self, invoice_id: InvoiceId) -> bool {
//        self.remote_invoice_id = match &self.remote_invoice_id {
//            &None => Some(invoice_id),
//            &Some(_) => return false,
//        };
//        true
//    }
//
//    pub fn decrease_balance(&mut self, payment: u128) {
//        // Possibly trim payment so that: local_pending_debt - balance < MAX_NEIGHBOR_DEBT
//        // This means that the sender of payment is losing some credits in this transaction.
//        let max_payment = cmp::max(MAX_NEIGHBOR_DEBT as i64 -
//            self.local_pending_debt as i64 +
//            self.balance as i64, 0) as u64;
//        let payment = cmp::min(payment, max_payment as u128) as u64;
//
//        // Apply payment to balance:
//        self.balance -= payment as i64;
//
//        // Possibly increase local_max_debt if we got too many credits:
//        self.local_max_debt = cmp::max(
//            self.local_max_debt as i64,
//            self.local_pending_debt as i64 - self.balance as i64)
//                as u64;
//    }
//
//    pub fn increase_remote_pending(&mut self, pending_credit: u64) -> bool {
//        if pending_credit as i64 >
//            (self.remote_max_debt as i64) - self.balance
//                - self.remote_max_debt as i64 {
//            self.remote_pending_debt += pending_credit;
//            true
//        } else {
//            false
//        }
//    }
//}
