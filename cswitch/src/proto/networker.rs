use std::convert::TryFrom;

use crypto::uuid::Uuid;
use crypto::rand_values::RandValue;
use crypto::identity::{PublicKey, Signature};

use proto::indexer::NeighborsRoute;

pub const CHANNEL_TOKEN_LEN: usize = 32;

/// The hash of the previous message sent over the token channel.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ChannelToken([u8; CHANNEL_TOKEN_LEN]);

#[derive(Clone, Copy, Debug)]
pub enum NeighborRequestType {
    CommMeans = 0,
    Encrypted = 1,
}

pub struct NeighborMoveToken {
    pub channel_index: u32,
    pub transactions:  Vec<NetworkerTokenChannelTransaction>,
    pub old_token:     ChannelToken,
    pub rand_nonce:    RandValue,
}

// TODO
pub enum NetworkerTokenChannelTransaction {
    SetRemoteMaximumDebt,
    FundsRandNonce,
    LoadFunds,
    RequestSendMessage {
        request_id: Uuid,
        route: NeighborsRoute,
        // request_content: RequestContent,
        maximum_response_length: u32,
        processing_fee_proposal: u64,
        half_credits_per_byte_proposal: u32,
    },
    ResponseSendMessage {
        request_id: Uuid,
        // response_content: ResponseContent,
        signature: Signature,
    },
    FailedSendMessage {
        request_id: Uuid,
        reporting_node_public_key: PublicKey,
        signature: Signature,
    },
    ResetChannel {
        new_balance: i128,
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