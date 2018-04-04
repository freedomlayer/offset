#![allow(dead_code, unused)]

mod types;
mod state;
mod error;

pub use self::state::HandshakeStateMachine;
pub type Result<T> = ::std::result::Result<T, error::HandshakeError>;
