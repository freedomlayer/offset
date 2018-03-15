use std::convert::TryFrom;

pub const CHANNEL_TOKEN_LEN: usize = 32;

/// The hash of the previous message sent over the token channel.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ChannelToken([u8; CHANNEL_TOKEN_LEN]);



// ========== Conversions ==========

impl AsRef<[u8]> for ChannelToken {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

impl<'a> TryFrom<&'a [u8]> for ChannelToken {
    type Error = ();

    fn try_from(src: &[u8]) -> Result<ChannelToken, Self::Error> {
        if src.len() != CHANNEL_TOKEN_LEN {
            Err(())
        } else {
            let mut inner = [0; CHANNEL_TOKEN_LEN];
            inner.clone_from_slice(src);
            Ok(ChannelToken(inner))
        }
    }
}
