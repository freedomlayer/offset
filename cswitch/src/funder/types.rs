pub const INVOICE_ID_LEN: usize = 32;

/// The Id number of an invoice.
///
/// An invoice is used during payment through the `Funder`. It is chosen by the sender of funds.
/// The invoice id then shows up in the receipt for the payment.
pub struct InvoiceId([u8; INVOICE_ID_LEN]);

impl InvoiceId {
    pub fn from_bytes(s: &[u8]) -> Result<InvoiceId, ()> {
        let mut inner = [0x00; INVOICE_ID_LEN];

        if s.len() == INVOICE_ID_LEN {
            inner.copy_from_slice(src);
            InvoiceId(inner)
        } else {
            Err(())
        }
    }
}

impl AsRef<[u8]> for InvoiceId {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

pub struct FriendMoveTokenMessage {
    pub transactions: Vec<FunderTokenChannelTransaction>,
    pub old_token:    Bytes,
    pub rand_nonce:   RandValue,
}

// TODO
pub enum FunderMoveTokenChannelTransaction {
    SetState,
    SetRemoteMaxDebt,
    RequestSendFund {
        request_id: Uuid,
        reoute:
    },
    ResponseSendFund,
    FailedSendFund,
    ResetChannel
}