/// Maximum amount of friend operations sent in one move token message.
pub const MAX_OPERATIONS_IN_BATCH: usize = 16;

/// Maximum length of route used to pass credit.
pub const MAX_ROUTE_LEN: usize = 32;

/// Amount of ticks to wait before rekeying a secure channel.
pub const TICKS_TO_REKEY: usize = 60 * 60 * 2; // 1 hour given that TICK is half a second.
