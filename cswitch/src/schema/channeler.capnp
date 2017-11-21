@0xa235992fe59d8f83;

# A custom made 128 bit data structure. This is used to overcome a limitation
# of capnproto to define large fixed sized arrays.
struct CustomUInt128 {
        x0 @0: UInt64;
        x1 @1: UInt64;
}

struct CustomUInt256 {
        x0 @0: UInt64;
        x1 @1: UInt64;
        x2 @2: UInt64;
        x3 @3: UInt64;
}

struct InitChannel {
        randValue @0: CustomUInt128;
}

# This is the structure of the encrypted_content of EncMessage:
struct PlainContent {
        recentRecipientRandValue @0: CustomUInt128;
        senderRandValue @1: CustomUInt128;
        messageContent @2: Data;
}

struct EncMessage {
        keySalt @0: CustomUInt256;
        encryptedContent @1: Data;
}
