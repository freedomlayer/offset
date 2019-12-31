#![deny(trivial_numeric_casts, warnings)]
#![allow(intra_doc_link_resolution_failure)]
#![allow(
    clippy::too_many_arguments,
    clippy::implicit_hasher,
    clippy::module_inception,
    clippy::new_without_default
)]

extern crate log;

mod app_conn;
mod connect;
mod identity;
mod types;

/// Utils for random generation of types
pub mod gen;

/// Utils for serializing and deserializing
pub mod ser_string {
    pub use common::ser_string::*;
    pub use proto::ser_string::*;
}

/// Common types
pub mod common {
    pub use crypto::identity::derive_public_key;
    pub use proto::app_server::messages::{NamedRelayAddress, RelayAddress};
    pub use proto::crypto::{
        HashResult, HashedLock, InvoiceId, PaymentId, PlainLock, PrivateKey, PublicKey, RandValue,
        Signature, Uid,
    };
    pub use proto::funder::messages::{
        Commit, Currency, FriendsRoute, PaymentStatus, PaymentStatusSuccess, Rate, Receipt,
    };
    pub use proto::index_server::messages::{
        MultiRoute, NamedIndexServerAddress, RouteCapacityRate,
    };
    pub use proto::net::messages::NetAddress;
}

/// Common Offst files:
pub use proto::file;

/// Offst connection
pub mod conn {
    pub use super::app_conn::{buyer, config, routes, seller};
    pub use super::connect::{connect, AppConnTuple, ConnPairApp, ConnectError};
    pub use super::identity::{identity_from_file, IdentityFromFileError};
    pub use proto::app_server::messages::{
        AppPermissions, AppRequest, AppServerToApp, AppToAppServer,
    };
    pub use proto::funder::messages::{RequestResult, ResponseClosePayment};
    pub use proto::index_client::messages::{ClientResponseRoutes, ResponseRoutesResult};
}

// TODO: Possibly reduce what we export from report in the future?
/// Report related types
pub mod report {
    pub use proto::report::messages::{
        AddFriendReport, ChannelConsistentReport, ChannelInconsistentReport, ChannelStatusReport,
        CurrencyConfigReport, CurrencyReport, FriendLivenessReport, FriendReport,
        FriendStatusReport, FunderReport, McBalanceReport, MoveTokenHashedReport,
        RequestsStatusReport, ResetTermsReport,
    };

    pub use proto::funder::messages::{
        BalanceInfo, CountersInfo, CurrencyBalance, CurrencyBalanceInfo, McInfo, TokenInfo,
    };

    pub use proto::app_server::messages::NodeReport;
    pub use proto::index_client::messages::{AddIndexServer, IndexClientReport};
}

/// Verification functions
pub mod verify {
    pub use signature::verify::{verify_commit, verify_move_token_hashed_report, verify_receipt};
}
