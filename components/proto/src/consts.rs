/// The current protocol version
pub const PROTOCOL_VERSION: u32 = 0;

/// Maximum amount of friend operations sent in one move token message.
pub const MAX_OPERATIONS_IN_BATCH: usize = 16;

/// Maximum length of route used to pass credit.
pub const MAX_ROUTE_LEN: usize = 32;

/// Maximum length for a name of a currency.
pub const MAX_CURRENCY_LEN: usize = 16;

// TODO: Possibly convert TICK_MS to be u64?
/// Amount of milliseconds in one tick:
pub const TICK_MS: usize = 1000;

/// Amount of ticks to wait before rekeying a secure channel.
pub const TICKS_TO_REKEY: usize = 60 * 60 * (1000 / TICK_MS); // 1 hour

/// If no message was sent for this amount of ticks, the connection will be closed
pub const KEEPALIVE_TICKS: usize = 0x20;

/// Relay server: The amount of ticks to wait before a relay connection from a client
/// sends identification of which type of connection it is.
pub const RELAY_CONN_TIMEOUT_TICKS: usize = 4;

/// The stream TCP connection is split into prefix length frames. This is the maximum allowed
/// length for such frame, measured in bytes.
pub const MAX_FRAME_LENGTH: usize = 1 << 20; // 1[MB]

/// Index server: The amount of ticks it takes for an idle node to be removed from the
/// index server database.
pub const INDEX_NODE_TIMEOUT_TICKS: usize = 60 * (1000 / TICK_MS); // 1 minute

/// Maximum length for an address string used in NetAddress
pub const MAX_NET_ADDRESS_LENGTH: usize = 256;

/// Maximum amount of relays a node may use.
/// We limit this number because sending many relays in a single move token message
/// might exceed frame length
pub const MAX_NODE_RELAYS: usize = 16;
