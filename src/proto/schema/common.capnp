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

# A custom type for a rational 64 bit number.
struct Ratio64 {
        union {
                one @0: Void;
                numerator @1: UInt64;
        }
}

# A custom type for a rational 128 bit number.
struct Ratio128 {
        union {
                one @0: Void;
                numerator @1: CustomUInt128;
        }
}


# A receipt for payment to the Funder
struct Receipt {
        responseHash @0: CustomUInt256;
        # = sha512/256(requestId || sha512/256(route) || randNonce)
        invoiceId @1: CustomUInt256;
        destPayment @2: CustomUInt128;
        signature @3: CustomUInt512;
        # Signature{key=recipientKey}(
        #   "FUND_SUCCESS" ||
        #   sha512/256(requestId || sha512/256(route) || randNonce) ||
        #   invoiceId ||
        #   destPayment
        # )
}
