#![allow(unused)]

use super::types::{TokenChannel, TcBalance, TcInvoice, TcSendPrice, TcIdents,
    TcPendingRequests, NeighborMoveTokenInner};


/// Processes outgoing messages for a token channel.
/// Used to batch as many messages as possible.
struct OutgoingTokenChannel {
    idents: TcIdents,
    balance: TcBalance,
    invoice: TcInvoice,
    send_price: TcSendPrice,
    pending_requests: TcPendingRequests,
}

/// A wrapper over a token channel, accumulating messages to be sent as one transcation.
impl OutgoingTokenChannel {
    pub fn new(token_channel: TokenChannel) -> OutgoingTokenChannel {
        OutgoingTokenChannel {
            idents: token_channel.idents,
            balance: token_channel.balance,
            invoice: token_channel.invoice,
            send_price: token_channel.send_price,
            pending_requests: token_channel.pending_requests,
        }
    }

    pub fn commit(self) -> (TokenChannel, NeighborMoveTokenInner) {
        // TODO
        unreachable!();
    }
}

