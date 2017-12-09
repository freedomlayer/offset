@0xa235992fe59d8f83;

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

struct InitChannel {
        neighborPublicKey @0: CustomUInt256;
        # The identity public key of the sender of this message.
        channelRandValue @1: CustomUInt128;
        # An initial random value.
        # This will be later used for the key exchange as a nonce.
        # A nonce is required to avoid replay of the signature.
}

struct Exchange {
        commPublicKey @0: CustomUInt256;
        # Communication public key (Generated for the new channel)
        # Using diffie-hellman we will use the communication keys to generate a
        # symmetric encryption key for this channel.
        keySalt @1: CustomUInt256;
        # A salt for the generation of a shared symmetric encryption key.
        signature @2: CustomUInt512;
        # Signature over (channelRandValue || commPublicKey || keySalt)
        # Signed using NeighborPublicKey.
}

enum MessageType {
        user @0;
        keepAlive @1;
}

struct EncryptMessage {
   incCounter  @0: UInt64;
   randPadding @1: Data;
   messageType @2: MessageType;
   content     @3: Data;
}

struct Message {
    content @0: Data;
    # The encrypt content of the EncryptMessage.
}