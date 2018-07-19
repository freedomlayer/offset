use super::LinearSendPrice;
// use crypto::rand_values::RandValue;

// use super::networker::ChannelToken;

pub const INVOICE_ID_LEN: usize = 32;

/// The universal unique identifier of an invoice.
define_fixed_bytes!(InvoiceId, INVOICE_ID_LEN);

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct FunderSendPrice(pub LinearSendPrice<u64>);


/*
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
*/
