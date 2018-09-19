@0xa7ec056ae12d5593;

using import "common.capnp".CustomUInt128;
using import "common.capnp".CustomUInt256;
using import "common.capnp".CustomUInt512;

# Diffie Hellman:
#################

struct ExchangeRandNonce {
    randNonce @0: CustomUInt128;
    publicKey @1: CustomUInt256;
}

struct ExchangeDh {
    dhPublicKey @0: CustomUInt256;
    randNonce @1: CustomUInt128;
    # This is the nonce previously sent by the remote side.
    keySalt @2: CustomUInt256;
    signature @3: CustomUInt512;
}

# Periodic rekeying is done inside the encrypted channel:
struct Rekey {
    dhPublicKey @0: CustomUInt256;
    keySalt @1: CustomUInt256;
}

struct ChannelMessage {
    randPadding @0: Data;
    content :union {
        keepAlive @1: Void;
        rekey     @2: Rekey;
        user      @3: Data;
    }
}
