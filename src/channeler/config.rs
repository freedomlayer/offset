//! The settings of `Channeler`.

pub(super) const RAND_VALUES_STORE_TICKS: usize = 5;
pub(super) const RAND_VALUES_STORE_CAPACITY: usize = 3;
pub(super) const REQUEST_NONCE_TIMEOUT: usize = 100;
pub(super) const HANDSHAKE_SESSION_TIMEOUT: usize = 100;
pub(super) const RECONNECT_INTERVAL: usize = 100;
pub(super) const MAXIMUM_RAND_PADDING_LEN: usize = 32; // Requirement: 2^16 %

pub(super) const MAXIMUM_CAROUSEL_RECEIVER: usize = 3;
pub(super) const CHANNEL_KEEPALIVE_TIMEOUT: usize = 100;
pub(super) const NONCE_WINDOW_WIDTH: usize = 256; // Requirement: % 64 == 0
