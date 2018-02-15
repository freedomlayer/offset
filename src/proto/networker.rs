use std::convert::TryFrom;

use crypto::uid::Uid;
use crypto::rand_values::RandValue;
use crypto::identity::{PublicKey, Signature};

use proto::indexer::NeighborsRoute;
use proto::common::SendFundsReceipt;

pub const CHANNEL_TOKEN_LEN: usize = 32;

/// The hash of the previous message sent over the token channel.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ChannelToken([u8; CHANNEL_TOKEN_LEN]);


pub struct NeighborMoveToken {
    pub channel_index: u32,
    pub transactions: Vec<NetworkerTokenChannelTransaction>,
    pub old_token: ChannelToken,
    pub rand_nonce: RandValue,
}

pub enum NetworkerTokenChannelTransaction {
    SetRemoteMaximumDebt(u64),
    FundsRandNonce(Uid),
    LoadFunds(SendFundsReceipt),
    RequestSendMessage {
        request_id: Uid,
        route: NeighborsRoute,
        request_content: Vec<u8>,
        max_response_len: u32,
        processing_fee_proposal: u64,
        credits_per_byte_proposal: u64,
    },
    ResponseSendMessage {
        request_id: Uid,
        rand_nonce: Uid,
        processing_fee_collected: u64,
        response_content: Vec<u8>,
        signature: Signature,
    },
    FailedSendMessage {
        request_id: Uid,
        reporting_public_key: PublicKey,
        rand_nonce: Uid,
        signature: Signature,
    },
    ResetChannel {
        new_balance: i64,
    },
}

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
