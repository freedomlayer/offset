#![feature(futures_api, async_await, await_macro, arbitrary_self_types)]
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
#![allow(unused)]

#[macro_use]
extern crate log;

mod connect;
pub mod gen;
mod identity;

pub use proto::file::friend::{load_friend_from_file, store_friend_to_file, FriendAddress};
pub use proto::file::index_server::load_index_server_from_file;
pub use proto::file::node::load_node_from_file;
pub use proto::file::relay::load_relay_from_file;
pub use proto::file::ser_string;

pub use proto::app_server::messages::{AppPermissions, NamedRelayAddress, RelayAddress};
pub use proto::funder::messages::Receipt;
pub use proto::funder::signature_buff::verify_receipt;
pub use proto::index_server::messages::NamedIndexServerAddress;
pub use proto::report::signature_buff::verify_move_token_hashed_report;

pub use node::connect::{AppConfig, AppReport, AppRoutes, AppSendFunds, NodeConnection};

pub use self::connect::{connect, ConnectError};
pub use self::identity::{identity_from_file, IdentityFromFileError};

// TODO: Possibly reduce what we export from report in the future?
pub mod report {
    pub use proto::report::messages::{
        AddFriendReport, ChannelInconsistentReport, ChannelStatusReport, DirectionReport,
        FriendLivenessReport, FriendReport, FriendReportMutation, FriendStatusReport, FunderReport,
        FunderReportMutateError, FunderReportMutation, FunderReportMutations, McBalanceReport,
        McRequestsStatusReport, MoveTokenHashedReport, RequestsStatusReport, ResetTermsReport,
        SentLocalRelaysReport, TcReport,
    };

    pub use proto::app_server::messages::{NodeReport, NodeReportMutation};
    pub use proto::index_client::messages::{
        AddIndexServer, IndexClientReport, IndexClientReportMutation,
    };
}

pub mod invoice {
    pub use crypto::invoice_id::{InvoiceId, INVOICE_ID_LEN};
}

pub mod route {
    pub use proto::funder::messages::FriendsRoute;
    pub use proto::index_server::messages::RouteWithCapacity;

}

pub use crypto::crypto_rand::{RandValue, RAND_VALUE_LEN};
pub use crypto::hash::{HashResult, HASH_RESULT_LEN};
pub use crypto::identity::{PublicKey, Signature, PUBLIC_KEY_LEN, SIGNATURE_LEN};
