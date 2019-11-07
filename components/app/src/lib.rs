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
mod identity;

/// Utils for random generation of types
pub mod gen;

/// Common types
pub mod common {
    pub use proto::app_server::messages::{NamedRelayAddress, RelayAddress};
    pub use proto::crypto::{
        HashResult, HashedLock, InvoiceId, PaymentId, PlainLock, PublicKey, RandValue, Signature,
        Uid,
    };
    pub use proto::funder::messages::{
        Commit, Currency, FriendsRoute, PaymentStatus, PaymentStatusSuccess, Rate, Receipt,
    };
    pub use proto::index_server::messages::{
        MultiRoute, NamedIndexServerAddress, RouteCapacityRate,
    };
}

/// Common Offst files:
pub use proto::file;

/// Utils for serializing and deserializing
pub use proto::ser_string;

/// Offst connection
pub mod conn {
    pub use super::app_conn::{AppBuyer, AppConfig, AppConn, AppReport, AppRoutes, AppSeller};
    pub use super::connect::{connect, node_connect, ConnectError};
    pub use super::identity::{identity_from_file, IdentityFromFileError};
}

// TODO: Possibly reduce what we export from report in the future?
/// Report related types
pub mod report {
    pub use proto::report::messages::{
        AddFriendReport, ChannelInconsistentReport, ChannelStatusReport, CurrencyReport,
        FriendLivenessReport, FriendReport, FriendStatusReport, FunderReport, McBalanceReport,
        McRequestsStatusReport, MoveTokenHashedReport, RequestsStatusReport, ResetTermsReport,
    };

    pub use proto::funder::messages::{
        BalanceInfo, CountersInfo, CurrencyBalanceInfo, McInfo, TokenInfo,
    };

    pub use proto::app_server::messages::NodeReport;
    pub use proto::index_client::messages::{AddIndexServer, IndexClientReport};
}

/// Verification functions
pub mod verify {
    pub use signature::verify::{verify_commit, verify_move_token_hashed_report, verify_receipt};
}
