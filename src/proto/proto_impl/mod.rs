#[macro_use]
mod macros;

pub mod common;
pub mod channeler;

pub mod app_manager {
    include_schema!(app_manager_capnp, "app_manager_capnp");
}
pub mod funder {
    include_schema!(funder_capnp, "funder_capnp");
}
pub mod networker {
    include_schema!(networker_capnp, "networker_capnp");
}
pub mod networker_crypter {
    include_schema!(networker_crypter_capnp, "networker_crypter_capnp");
}
