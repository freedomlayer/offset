@0xbc544d9c10380608;

using import "common.capnp".CustomUInt128;
using import "common.capnp".CustomUInt256;
using import "common.capnp".CustomUInt512;


# 4-way handshake
# ---------------
# 1. A -> B: InitChannel      (randNonceA, publicKeyA)
# 2. B -> A: ExchangePassive  (hashPrev, randNonceB, publicKeyB, dhPublicKeyB, keySaltB, Signature(B))
# 3. A -> B: ExchangeActive   (hashPrev, dhPublicKeyA, keySaltA, Signature(A))
# 4. B -> A: ChannelReady     (hashPrev, Signature(B))


struct InitChannel {
        randNonce1 @0: CustomUInt128;
        publicKey1 @1: CustomUInt256;
}

struct ExchangePassive {
        hashPrev @0: CustomUInt256;
        randNonce2 @1: CustomUInt128;
        publicKey2 @2: CustomUInt256;
        dhPublicKey2 @3: CustomUInt256;
        keySalt2 @4: CustomUInt256;
        signature @5: CustomUInt512;
        # Signature is applied for all the previous fields
        # Signature("ExchangePassive" || fields ...)
}

struct ExchangeActive {
        hashPrev @0: CustomUInt256;
        dhPublicKey1 @1: CustomUInt256;
        keySalt1 @2: CustomUInt256;
        signature @3: CustomUInt512;
        # Signature is applied for all the previous fields
        # Signature("ExchangeActive" || fields ...)
}

struct ChannelReady {
        hashPrev @0: CustomUInt256;
        signature @1: CustomUInt512;
        # Signature is applied for all the previous fields
        # Signature("ChannelReady" || fields ...)
}


struct UnknownChannel {
        channelId @0: CustomUInt128;
        randNonce @1: CustomUInt128;
        signature @2: CustomUInt512;
        # Signature is applied for all the previous fields.
        # Signature("UnknownChannel" || fields ...)
}


# The data we encrypt. Contains random padding (Of variable length) together
# with actual data. This struct is used for any data we encrypt.
struct Plain {
   randPadding @0: Data;
   # Random padding: Makes traffic analysis harder.
   content @1: Data;
}


struct ChannelerMessage {
        union {
                initChannel @0: InitChannel;
                exchangePassive @1: ExchangePassive;
                exchangeActive @2: ExchangeActive;
                channelReady @3: ChannelReady;
                unknownChannel @4: UnknownChannel;
                encrypted @5: Data;
                # Nonce is a 96 bit counter, Additional authenticated data is
                # channelId.
        }
}
