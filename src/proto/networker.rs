pub const CHANNEL_TOKEN_LEN: usize = 32;

/// The hash of the previous message sent over the token channel.
define_fixed_bytes!(ChannelToken, CHANNEL_TOKEN_LEN);


#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LinearSendPrice<T> {
    base: T,
    multiplier: T,
}

pub type NetworkerSendPrice = LinearSendPrice<u32>;
