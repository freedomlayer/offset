use crypto::rand_values::RandValue;

use super::networker::ChannelToken;

pub const INVOICE_ID_LEN: usize = 32;

/// The universal unique identifier of an invoice.
///
/// An invoice is used during payment through the `Funder`. It is chosen by the sender of funds.
/// The invoice id then shows up in the receipt for the payment.
#[derive(Clone, Eq, PartialEq)]
pub struct InvoiceId([u8; INVOICE_ID_LEN]);

impl InvoiceId {
    pub fn from_bytes(src: &[u8]) -> Result<InvoiceId, ()> {
        if src.len() != INVOICE_ID_LEN {
            Err(())
        } else {
            let mut inner = [0x00; INVOICE_ID_LEN];
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
    ResetChannel,
}
