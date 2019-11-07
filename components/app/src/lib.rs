#![feature(arbitrary_self_types)]
#![feature(nll)]
#![feature(generators)]
#![feature(never_type)]
#![deny(trivial_numeric_casts, warnings)]
#![allow(intra_doc_link_resolution_failure)]
#![allow(
    clippy::too_many_arguments,
    clippy::implicit_hasher,
    clippy::module_inception,
    clippy::new_without_default
)]

#[macro_use]
extern crate log;

mod app_conn;
mod connect;
pub mod gen;
mod identity;

pub use proto::file;
pub use proto::ser_string;

pub use proto::app_server::messages::{AppPermissions, NamedRelayAddress, RelayAddress};
pub use proto::funder::messages::{
    BalanceInfo, Commit, CountersInfo, Currency, CurrencyBalanceInfo, McInfo, PaymentStatus,
    PaymentStatusSuccess, Rate, Receipt, TokenInfo,
};
pub use proto::index_server::messages::NamedIndexServerAddress;

pub use signature::verify::{verify_commit, verify_move_token_hashed_report, verify_receipt};

pub use self::app_conn::{AppBuyer, AppConfig, AppConn, AppReport, AppRoutes, AppSeller};

pub use self::connect::{connect, node_connect, ConnectError};
pub use self::identity::{identity_from_file, IdentityFromFileError};

// TODO: Possibly reduce what we export from report in the future?
pub mod report {
    pub use proto::report::messages::{
        AddFriendReport, ChannelInconsistentReport, ChannelStatusReport, CurrencyReport,
        FriendLivenessReport, FriendReport, FriendReportMutation, FriendStatusReport, FunderReport,
        FunderReportMutateError, FunderReportMutation, FunderReportMutations, McBalanceReport,
        McRequestsStatusReport, MoveTokenHashedReport, RequestsStatusReport, ResetTermsReport,
    };

    pub use proto::app_server::messages::{NodeReport, NodeReportMutation};
    pub use proto::index_client::messages::{
        AddIndexServer, IndexClientReport, IndexClientReportMutation,
    };
}

pub use proto::crypto;
pub mod route {
    pub use proto::funder::messages::FriendsRoute;
    pub use proto::index_server::messages::{MultiRoute, RouteCapacityRate};
}

/*
pub mod invoice {
    pub use crypto::invoice_id::{InvoiceId, InvoiceId::len()};
}

pub mod payment {
    pub use crypto::payment_id::{PaymentId, PaymentId::len()};
}


pub use crypto::hash::{HashResult, HASH_RESULT_LEN};
pub use crypto::hash_lock::{HashedLock, PlainLock, HASHED_LOCK_LEN, PlainLock::len()};
pub use crypto::identity::{PublicKey, Signature, PublicKey::len(), Signature::len()};
pub use crypto::rand::{RandValue, RandValue::len()};
*/
