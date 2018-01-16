use crypto::rand_values::RandValue;

pub const INVOICE_ID_LEN: usize = 32;
pub const CHANNEL_TOKEN_LEN: usize = 32;

/// The hash of the previous message sent over the token channel.
pub struct ChannelToken([u8; CHANNEL_TOKEN_LEN]);

/// The universal unique identifier of an invoice.
///
/// An invoice is used during payment through the `Funder`. It is chosen by the sender of funds.
/// The invoice id then shows up in the receipt for the payment.
pub struct InvoiceId([u8; INVOICE_ID_LEN]);

impl InvoiceId {
    pub fn from_bytes(src: &[u8]) -> Result<InvoiceId, ()> {
        let mut inner = [0x00; INVOICE_ID_LEN];

        if src.len() != INVOICE_ID_LEN {
            Err(())
        } else {
            inner.copy_from_slice(src);
            Ok(InvoiceId(inner))
        }
    }
}

impl AsRef<[u8]> for InvoiceId {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

pub struct FriendMoveToken {
    pub transactions: Vec<FunderTokenChannelTransaction>,
    pub old_token: ChannelToken,
    pub rand_nonce: RandValue,
}

// TODO
pub enum FunderTokenChannelTransaction {
    SetState,
    SetRemoteMaxDebt,
//    RequestSendFund {
//        request_id: Uuid,
//        reoute:
//    },
    ResponseSendFund,
    FailedSendFund,
    ResetChannel
}