//! The

mod types;
mod state;
mod error;

pub use self::types::HandshakeResult;
pub use self::state::Handshaker;


pub type Result<T> = ::std::result::Result<T, error::HandshakeError>;
pub type SharedRandom = ::std::rc::Rc<::ring::rand::SecureRandom>;
