@0xa235992fe59d8f83;

using import "common.capnp".CustomUInt128;
using import "common.capnp".CustomUInt256;
using import "common.capnp".CustomUInt512;

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
