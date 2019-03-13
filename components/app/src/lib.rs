#![feature(futures_api, async_await, await_macro, arbitrary_self_types)]
#![feature(nll)]
#![feature(generators)]
#![feature(never_type)]

#![deny(
    trivial_numeric_casts,
    warnings
)]

#![allow(unused)]

#[macro_use]
extern crate log;

mod identity;
mod connect;
pub mod uid;

pub use proto::file::node::load_node_from_file;
pub use self::identity::{identity_from_file, IdentityFromFileError};
pub use self::connect::{connect, ConnectError};
pub use node::connect::{AppReport, AppConfig, 
    AppRoutes, AppSendFunds, NodeConnection};
pub use proto::app_server::messages::AppPermissions;


// TODO: Possibly reduce what we export from report in the future?
pub mod report {
    pub use proto::report::messages::{ChannelStatusReport,
        MoveTokenHashedReport, SentLocalRelaysReport,
        FriendStatusReport, RequestsStatusReport, McRequestsStatusReport,
        McBalanceReport, DirectionReport, FriendLivenessReport, TcReport,
        ResetTermsReport, ChannelInconsistentReport, FriendReport, FunderReport,
        FriendReportMutation, AddFriendReport, FunderReportMutation, 
        FunderReportMutations, FunderReportMutateError};

    use proto::app_server::messages::{NodeReport, NodeReportMutation};
    use proto::index_client::messages::{IndexClientReport, AddIndexServer, IndexClientReportMutation};
}

pub mod invoice {
    pub use proto::funder::messages::{InvoiceId, INVOICE_ID_LEN};
}
