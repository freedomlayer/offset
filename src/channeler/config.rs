//! The settings of `Channeler`.

pub(super) const REQUEST_NONCE_TIMEOUT: usize = 100;
pub(super) const HANDSHAKE_SESSION_TIMEOUT: usize = 100;

pub(super) const MAXIMUM_CAROUSEL_RECEIVER: usize = 3;
pub(super) const CHANNEL_KEEPALIVE_TIMEOUT: usize = 100;
pub(super) const NONCE_WINDOW_WIDTH: usize = 256; // Requirement: % 64 == 0
