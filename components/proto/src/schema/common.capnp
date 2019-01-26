@0xbcd851c16e3f44ce;

# A custom made 128 bit data structure.
struct Buffer128 {
        x0 @0: UInt64;
        x1 @1: UInt64;
}

# A custom made 256 bit data structure.
struct Buffer256 {
        x0 @0: UInt64;
        x1 @1: UInt64;
        x2 @2: UInt64;
        x3 @3: UInt64;
}

# A custom made 512 bit data structure.
struct Buffer512 {
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
        inner @0: Buffer256;
}

struct DhPublicKey {
        inner @0: Buffer256;
}

struct Signature {
        inner @0: Buffer512;
}

struct RandNonce {
        inner @0: Buffer128;
}

# Unsigned 128 bit integer
struct CustomUInt128 {
        inner @0: Buffer128;
}

# Signed 128 bit integer
struct CustomInt128 {
        inner @0: Buffer128;
}


struct Hash {
        inner @0: Buffer256;
}

struct InvoiceId {
        inner @0: Buffer256;
}

struct Salt {
        inner @0: Buffer256;
}

struct Uid {
        inner @0: Buffer128;
}


# A receipt for payment to the Funder
struct Receipt {
        responseHash @0: Hash;
        # = sha512/256(requestId || sha512/256(route) || randNonce)
        invoiceId @1: InvoiceId;
        destPayment @2: CustomUInt128;
        signature @3: Signature;
        # Signature{key=recipientKey}(
        #   "FUND_SUCCESS" ||
        #   sha512/256(requestId || sha512/256(route) || randNonce) ||
        #   invoiceId ||
        #   destPayment
        # )
}

# Ipv4 TCP address
struct TcpAddressV4 {
        address @0: UInt32;
        port @1: UInt16;
}

# Ipv6 TCP address
struct TcpAddressV6 {
        address @0: Buffer128;
        port @1: UInt16;
}

struct TcpAddress {
        union {
                v4 @0: TcpAddressV4;
                v6 @1: TcpAddressV6;
        }
}

# Authenticated address of a Relay (Includes relay's public key)
struct RelayAddress {
        publicKey @0: PublicKey;
        address @1: TcpAddress;
}

