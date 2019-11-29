@0xa7ec056ae12d5593;

using import "common.capnp".PublicKey;
using import "common.capnp".DhPublicKey;
using import "common.capnp".Salt;
using import "common.capnp".Signature;
using import "common.capnp".RandValue;

# Diffie Hellman:
#################

struct ExchangeRandNonce {
    randNonce @0: RandValue;
    srcPublicKey @1: PublicKey;
    # Sender's public key
    optDestPublicKey: union {
            empty @2: Void;
            # Nothing has changed
            publicKey @3: PublicKey;
            # Set this exact list to be the list of relays
    }
    # PublicKey the sender expects the remote side to have.
    # Useful for multiplexing multiple entities behind one listening port.
    # A multiplexer can identify right at the first incoming message to which
    # entity should this connection be redirected.
}

struct ExchangeDh {
    dhPublicKey @0: DhPublicKey;
    randNonce @1: RandValue;
    # This is the nonce previously sent by the remote side.
    keySalt @2: Salt;
    signature @3: Signature;
}

# Periodic rekeying is done inside the encrypted channel:
struct Rekey {
    dhPublicKey @0: DhPublicKey;
    keySalt @1: Salt;
}

struct ChannelContent {
        union {
                rekey @0: Rekey;
                user @1: Data;
        }
}

struct ChannelMessage {
    randPadding @0: Data;
    content @1: ChannelContent;    
}
