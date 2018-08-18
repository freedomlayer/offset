// use crypto::rand_values::RandValue;

// use super::networker::ChannelToken;

pub const INVOICE_ID_LEN: usize = 32;

/// The universal unique identifier of an invoice.
define_fixed_bytes!(InvoiceId, INVOICE_ID_LEN);

pub const CHANNEL_TOKEN_LEN: usize = 32;

/// The hash of the previous message sent over the token channel.
define_fixed_bytes!(ChannelToken, CHANNEL_TOKEN_LEN);

