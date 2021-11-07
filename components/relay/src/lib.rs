#![crate_type = "lib"]
#![type_length_limit = "291239"]
#![deny(trivial_numeric_casts, warnings)]
#![allow(broken_intra_doc_links)]
#![allow(
    clippy::too_many_arguments,
    clippy::implicit_hasher,
    clippy::module_inception,
    clippy::new_without_default
)]

#[macro_use]
extern crate log;

#[macro_use]
extern crate common;

mod client;
mod server;

pub use self::client::client_connector::ClientConnector;
pub use self::client::client_listener::ClientListener;
pub use self::server::{relay_server, RelayServerError};
