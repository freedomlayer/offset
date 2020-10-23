#![crate_type = "lib"]
#![deny(trivial_numeric_casts, warnings)]
#![allow(broken_intra_doc_links)]
#![allow(
    clippy::too_many_arguments,
    clippy::implicit_hasher,
    clippy::module_inception,
    clippy::new_without_default
)]

mod multi_route;

pub use multi_route::{choose_multi_route, MultiRouteChoice};
