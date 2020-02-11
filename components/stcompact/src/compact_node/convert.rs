use app::common::Currency;

use crate::compact_node::messages::{
    BalanceInfo, ChannelConsistentReport, ChannelInconsistentReport, ChannelStatusReport, Commit,
    CompactReport, ConfigReport, CountersInfo, CurrencyReport, FriendLivenessReport, FriendReport,
    FriendStatusReport, McInfo, MoveTokenHashedReport, OpenInvoice, OpenPayment, OpenPaymentStatus,
    RequestsStatusReport, ResetTermsReport, TokenInfo,
};

use crate::compact_node::persist;
use crate::compact_node::persist::CompactState;

/// A helper util struct, used to implement From for structures that are not from this crate,
/// mostly for ad-hoc tuples used for type conversion.
#[derive(Debug, Clone)]
struct LocalWrapper<T>(pub T);

// ====================[NodeReport]============================

impl From<app::report::CurrencyReport> for LocalWrapper<(Currency, CurrencyReport)> {
    fn from(from: app::report::CurrencyReport) -> Self {
        LocalWrapper((
            from.currency,
            CurrencyReport {
                balance: from.balance.balance,
                local_pending_debt: from.balance.local_pending_debt,
                remote_pending_debt: from.balance.remote_pending_debt,
            },
        ))
    }
}

impl From<app::report::ChannelConsistentReport> for ChannelConsistentReport {
    fn from(from: app::report::ChannelConsistentReport) -> Self {
        ChannelConsistentReport {
            currency_reports: from
                .currency_reports
                .into_iter()
                .map(|currency_report| {
                    LocalWrapper::<(Currency, CurrencyReport)>::from(currency_report).0
                })
                .collect(),
        }
    }
}

impl From<app::report::CurrencyBalance> for LocalWrapper<(Currency, i128)> {
    fn from(from: app::report::CurrencyBalance) -> Self {
        LocalWrapper((from.currency, from.balance))
    }
}

impl From<app::report::ResetTermsReport> for ResetTermsReport {
    fn from(from: app::report::ResetTermsReport) -> Self {
        ResetTermsReport {
            reset_token: from.reset_token,
            balance_for_reset: from
                .balance_for_reset
                .into_iter()
                .map(|currency_balance| LocalWrapper::<(Currency, i128)>::from(currency_balance).0)
                .collect(),
        }
    }
}

impl From<app::report::ChannelInconsistentReport> for ChannelInconsistentReport {
    fn from(from: app::report::ChannelInconsistentReport) -> Self {
        ChannelInconsistentReport {
            local_reset_terms: from
                .local_reset_terms
                .into_iter()
                .map(|currency_balance| LocalWrapper::<(Currency, i128)>::from(currency_balance).0)
                .collect(),
            opt_remote_reset_terms: from
                .opt_remote_reset_terms
                .map(|reset_terms_report| reset_terms_report.into()),
        }
    }
}

impl From<app::report::ChannelStatusReport> for ChannelStatusReport {
    fn from(from: app::report::ChannelStatusReport) -> Self {
        match from {
            app::report::ChannelStatusReport::Consistent(channel_consistent_report) => {
                ChannelStatusReport::Consistent(channel_consistent_report.into())
            }
            app::report::ChannelStatusReport::Inconsistent(channel_inconsistent_report) => {
                ChannelStatusReport::Inconsistent(channel_inconsistent_report.into())
            }
        }
    }
}

impl From<app::report::CurrencyBalanceInfo> for LocalWrapper<(Currency, BalanceInfo)> {
    fn from(from: app::report::CurrencyBalanceInfo) -> Self {
        LocalWrapper((
            from.currency,
            BalanceInfo {
                balance: from.balance_info.balance,
                local_pending_debt: from.balance_info.local_pending_debt,
                remote_pending_debt: from.balance_info.remote_pending_debt,
            },
        ))
    }
}

impl From<app::report::McInfo> for McInfo {
    fn from(from: app::report::McInfo) -> Self {
        McInfo {
            local_public_key: from.local_public_key,
            remote_public_key: from.remote_public_key,
            balances: from
                .balances
                .into_iter()
                .map(|currency_balance_info| {
                    LocalWrapper::<(Currency, BalanceInfo)>::from(currency_balance_info).0
                })
                .collect(),
        }
    }
}

impl From<app::report::CountersInfo> for CountersInfo {
    fn from(from: app::report::CountersInfo) -> Self {
        CountersInfo {
            inconsistency_counter: from.inconsistency_counter,
            move_token_counter: from.move_token_counter,
        }
    }
}

impl From<app::report::TokenInfo> for TokenInfo {
    fn from(from: app::report::TokenInfo) -> Self {
        TokenInfo {
            mc: from.mc.into(),
            counters: from.counters.into(),
        }
    }
}

impl From<app::report::MoveTokenHashedReport> for MoveTokenHashedReport {
    fn from(from: app::report::MoveTokenHashedReport) -> Self {
        MoveTokenHashedReport {
            prefix_hash: from.prefix_hash,
            token_info: from.token_info.into(),
            rand_nonce: from.rand_nonce,
            new_token: from.new_token,
        }
    }
}

impl From<app::report::FriendStatusReport> for FriendStatusReport {
    fn from(friend_status_report: app::report::FriendStatusReport) -> Self {
        match friend_status_report {
            app::report::FriendStatusReport::Enabled => FriendStatusReport::Enabled,
            app::report::FriendStatusReport::Disabled => FriendStatusReport::Disabled,
        }
    }
}

impl From<app::report::FriendLivenessReport> for FriendLivenessReport {
    fn from(friend_liveness_report: app::report::FriendLivenessReport) -> Self {
        match friend_liveness_report {
            app::report::FriendLivenessReport::Online => FriendLivenessReport::Online,
            app::report::FriendLivenessReport::Offline => FriendLivenessReport::Offline,
        }
    }
}

impl From<app::report::RequestsStatusReport> for RequestsStatusReport {
    fn from(requests_status_report: app::report::RequestsStatusReport) -> Self {
        match requests_status_report {
            app::report::RequestsStatusReport::Closed => RequestsStatusReport::Closed,
            app::report::RequestsStatusReport::Open => RequestsStatusReport::Open,
        }
    }
}

impl From<app::report::CurrencyConfigReport> for LocalWrapper<(Currency, ConfigReport)> {
    fn from(currency_config_report: app::report::CurrencyConfigReport) -> Self {
        LocalWrapper((
            currency_config_report.currency,
            ConfigReport {
                rate: currency_config_report.rate,
                remote_max_debt: currency_config_report.remote_max_debt,
                is_open: currency_config_report.is_open,
            },
        ))
    }
}

impl From<app::report::FriendReport> for FriendReport {
    fn from(friend_report: app::report::FriendReport) -> Self {
        FriendReport {
            name: friend_report.name,
            currency_configs: friend_report
                .currency_configs
                .into_iter()
                .map(|currency_config_report| {
                    LocalWrapper::<(Currency, ConfigReport)>::from(currency_config_report).0
                })
                .collect(),
            opt_last_incoming_move_token: friend_report
                .opt_last_incoming_move_token
                .map(|last_incoming_move_token| last_incoming_move_token.into()),
            liveness: friend_report.liveness.into(),
            channel_status: friend_report.channel_status.into(),
            status: friend_report.status.into(),
        }
    }
}

// ====================[Commit]==============================

impl From<app::common::Commit> for Commit {
    fn from(from: app::common::Commit) -> Self {
        Commit {
            response_hash: from.response_hash,
            src_plain_lock: from.src_plain_lock,
            dest_hashed_lock: from.dest_hashed_lock,
            dest_payment: from.dest_payment,
            total_dest_payment: from.total_dest_payment,
            invoice_id: from.invoice_id,
            currency: from.currency,
            signature: from.signature,
        }
    }
}

impl From<Commit> for app::common::Commit {
    fn from(from: Commit) -> Self {
        app::common::Commit {
            response_hash: from.response_hash,
            src_plain_lock: from.src_plain_lock,
            dest_hashed_lock: from.dest_hashed_lock,
            dest_payment: from.dest_payment,
            total_dest_payment: from.total_dest_payment,
            invoice_id: from.invoice_id,
            currency: from.currency,
            signature: from.signature,
        }
    }
}

// ==================[CompactReport]==========================
//

impl From<persist::OpenPaymentStatus> for OpenPaymentStatus {
    fn from(from: persist::OpenPaymentStatus) -> Self {
        match from {
            persist::OpenPaymentStatus::SearchingRoute(request_routes_id) => {
                OpenPaymentStatus::SearchingRoute(request_routes_id)
            }
            persist::OpenPaymentStatus::FoundRoute(found_route) => {
                OpenPaymentStatus::FoundRoute(found_route.confirm_id, found_route.fees)
            }
            persist::OpenPaymentStatus::Sending(sending) => {
                OpenPaymentStatus::Sending(sending.fees)
            }
            persist::OpenPaymentStatus::Commit(commit, fees) => {
                OpenPaymentStatus::Commit(commit.into(), fees)
            }
            persist::OpenPaymentStatus::Success(receipt, fees, ack_uid) => {
                OpenPaymentStatus::Success(receipt, fees, ack_uid)
            }
            persist::OpenPaymentStatus::Failure(ack_uid) => OpenPaymentStatus::Failure(ack_uid),
        }
    }
}

impl From<persist::OpenPayment> for OpenPayment {
    fn from(from: persist::OpenPayment) -> Self {
        OpenPayment {
            invoice_id: from.invoice_id,
            currency: from.currency,
            dest_public_key: from.dest_public_key,
            dest_payment: from.dest_payment,
            description: from.description,
            generation: from.generation,
            status: from.status.into(),
        }
    }
}

impl From<persist::OpenInvoice> for OpenInvoice {
    fn from(from: persist::OpenInvoice) -> Self {
        OpenInvoice {
            currency: from.currency,
            total_dest_payment: from.total_dest_payment,
            description: from.description,
            generation: from.generation,
        }
    }
}

pub fn create_compact_report(
    compact_state: CompactState,
    node_report: app::report::NodeReport,
) -> CompactReport {
    CompactReport {
        local_public_key: node_report.funder_report.local_public_key,
        index_servers: node_report.index_client_report.index_servers,
        opt_connected_index_server: node_report.index_client_report.opt_connected_server,
        relays: node_report.funder_report.relays,
        friends: node_report
            .funder_report
            .friends
            .into_iter()
            .map(|(friend_public_key, friend_report)| (friend_public_key, friend_report.into()))
            .collect(),
        open_invoices: compact_state
            .open_invoices
            .into_iter()
            .map(|(invoice_id, open_invoice)| (invoice_id, open_invoice.into()))
            .collect(),
        open_payments: compact_state
            .open_payments
            .into_iter()
            .map(|(payment_id, open_payment)| (payment_id, open_payment.into()))
            .collect(),
    }
}
