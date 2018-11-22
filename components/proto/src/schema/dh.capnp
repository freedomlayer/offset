@0xa7ec056ae12d5593;

using import "common.capnp".PublicKey;
using import "common.capnp".DhPublicKey;
using import "common.capnp".Salt;
using import "common.capnp".Signature;
using import "common.capnp".RandNonce;

# Diffie Hellman:
#################

struct ExchangeRandNonce {
    randNonce @0: RandNonce;
    publicKey @1: PublicKey;
}

struct ExchangeDh {
    dhPublicKey @0: DhPublicKey;
    randNonce @1: RandNonce;
    # This is the nonce previously sent by the remote side.
    keySalt @2: Salt;
    signature @3: Signature;
}

# Periodic rekeying is done inside the encrypted channel:
struct Rekey {
    dhPublicKey @0: DhPublicKey;
    keySalt @1: Salt;
}

struct ChannelMessage {
    randPadding @0: Data;
    content :union {
        rekey     @1: Rekey;
        user      @2: Data;
    }
}
