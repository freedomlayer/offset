@0xbcd851c16e3f44ce;

# A custom made 128 bit data structure.
struct CustomUInt128 {
        x0 @0: UInt64;
        x1 @1: UInt64;
}

# A custom made 256 bit data structure.
struct CustomUInt256 {
        x0 @0: UInt64;
        x1 @1: UInt64;
        x2 @2: UInt64;
        x3 @3: UInt64;
}

# A custom made 512 bit data structure.
struct CustomUInt512 {
        x0 @0: UInt64;
        x1 @1: UInt64;
        x2 @2: UInt64;
        x3 @3: UInt64;
        x4 @4: UInt64;
        x5 @5: UInt64;
        x6 @6: UInt64;
        x7 @7: UInt64;
}

# A pair of a nonce and a signature used to sign a FailedSendXXXTran.
# Every party on the route back to the originator of the request message add his 
# signature over the failure message. This is important to avoid the hollow failure route attack.
struct RandNonceSignature {
        randNonce @0: CustomUInt128;
        signature @1: CustomUInt512;
}


# A receipt for payment to the Funder
struct Receipt {
        responseHash @0: CustomUInt256;
        # = sha512/256(requestId || 
        #       sha512/256(nodeIdPath) || 
        #       mediatorPaymentProposal)
        invoiceId @1: CustomUInt256;
        destinationPayment @2: CustomUInt128;
        randNonce @3: CustomUInt256;
        signature @4: CustomUInt512;
        # Signature{key=recipientKey}(
        #   "FUND_SUCCESS" ||
        #   sha512/256(requestId || sha512/256(nodeIdPath) || mediatorPaymentProposal) ||
        #   invoiceId ||
        #   destinationPayment ||
        #   randNonce)
}

