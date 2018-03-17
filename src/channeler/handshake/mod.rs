#![allow(dead_code, unused)]

mod types;
mod state_machine;

pub type Result<T> = ::std::result::Result<T, types::HandshakeError>;
