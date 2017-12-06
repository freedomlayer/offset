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
        # An initial random value. This will be later used for the key exchange as a nonce.
        # A nonce is required to avoid replay of the signature.
}

struct Exchange {
        commPublicKey @0: CustomUInt256;
        # Communication public key (Generated for the new channel)
        # Using diffie-hellman we will use the communication keys to generate a
        # symmetric encryption key for this channel.
        keySalt @1: CustomUInt256;
        # A salt for the generation of a shared symmetric encryption key.
        # senderRandValue @2: CustomUInt128;
        # This is the first senderRandValue. It will be used by the remote
        # party to send messages to us on this channel.
        signature @2: CustomUInt512;
        # Signature over (channelRandValue || commPublicKey || keySalt || senderRandValue)
        # Signed using NeighborPublicKey.
}

# Contents for a keepalive message:
struct KeepaliveContent {
        # serialNumber @0: Uint64;
        # recentRecipientRandValue @0: CustomUInt128;
        # senderRandValue @1: CustomUInt128;
}

# This is the structure of the encrypted_content of EncMessage:
struct PlainContent {
        serialNumber @0: UInt64;
        messageContent @1: Data;
}

struct EncMessage {
        encryptedContent @0: Data;
}

# Message container:
enum MessageType {
        initChannel @0;
        exchange @1;
        keepaliveMessage @2;
        encMessage @3;
}

struct Message {
        messageType @0: MessageType;
        message @1: Data;
}
