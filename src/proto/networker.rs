use std::mem;
pub const CHANNEL_TOKEN_LEN: usize = 32;

/// The hash of the previous message sent over the token channel.
define_fixed_bytes!(ChannelToken, CHANNEL_TOKEN_LEN);


#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LinearSendPrice<T> {
    base: T,
    multiplier: T,
}

impl<T> LinearSendPrice<T> {
    pub fn bytes_count() -> usize {
        mem::size_of::<T>() * 2
    }

}

pub type NetworkerSendPrice = LinearSendPrice<u32>;
