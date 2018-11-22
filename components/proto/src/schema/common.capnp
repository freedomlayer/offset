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

struct PublicKey {
        inner @0: CustomUInt256;
}

struct DhPublicKey {
        inner @0: CustomUInt256;
}

struct Signature {
        inner @0: CustomUInt512;
}

struct RandNonce {
        inner @0: CustomUInt128;
}

struct Balance {
        inner @0: CustomUInt128;
}

struct Debt {
        inner @0: CustomUInt128;
}

struct Hash {
        inner @0: CustomUInt256;
}

struct InvoiceId {
        inner @0: CustomUInt256;
}

struct Salt {
        inner @0: CustomUInt256;
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
        responseHash @0: Hash;
        # = sha512/256(requestId || sha512/256(route) || randNonce)
        invoiceId @1: InvoiceId;
        destPayment @2: Debt;
        signature @3: Signature;
        # Signature{key=recipientKey}(
        #   "FUND_SUCCESS" ||
        #   sha512/256(requestId || sha512/256(route) || randNonce) ||
        #   invoiceId ||
        #   destPayment
        # )
}
