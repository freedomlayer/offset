@0xbc544d9c10380608;

using import "common.capnp".Buffer128;
using import "common.capnp".Buffer256;
using import "common.capnp".Buffer512;

# 4-way handshake
# ---------------
# 1. A -> B: InitChannel      (randNonceA, publicKeyA)
# 2. B -> A: ExchangePassive  (hashPrev, randNonceB, publicKeyB, dhPublicKeyB, keySaltB, Signature(B))
# 3. A -> B: ExchangeActive   (hashPrev, dhPublicKeyA, keySaltA, Signature(A))
# 4. B -> A: ChannelReady     (hashPrev, Signature(B))

struct InitChannel {
    randNonce @0: Buffer128;
    publicKey @1: Buffer256;
}

struct ExchangePassive {
    prevHash    @0: Buffer256;
    randNonce   @1: Buffer128;
    publicKey   @2: Buffer256;
    dhPublicKey @3: Buffer256;
    keySalt     @4: Buffer256;
    signature   @5: Buffer512;
    # Signature is applied for all the previous fields
    # Signature("ExchangePassive" || fields ...)
}

struct ExchangeActive {
        prevHash    @0: Buffer256;
        dhPublicKey @1: Buffer256;
        keySalt     @2: Buffer256;
        signature   @3: Buffer512;
        # Signature is applied for all the previous fields
        # Signature("ExchangeActive" || fields ...)
}

struct ChannelReady {
    prevHash  @0: Buffer256;
    signature @1: Buffer512;
    # Signature is applied for all the previous fields
    # Signature("ChannelReady" || fields ...)
}

struct UnknownChannel {
    channelId @0: Buffer128;
    randNonce @1: Buffer128;
    signature @2: Buffer512;
    # Signature is applied for all the previous fields.
    # Signature("UnknownChannel" || fields ...)
}

# The data we encrypt. Contains random padding (Of variable length) together
# with actual data. This struct is used for any data we encrypt.
struct Plain {
    randPadding  @0: Data;
    content :union {
        user      @1: Data;
        keepAlive @2: Void;
    }
}

struct ChannelerMessage {
    union {
        initChannel     @0: InitChannel;
        exchangePassive @1: ExchangePassive;
        exchangeActive  @2: ExchangeActive;
        channelReady    @3: ChannelReady;
        unknownChannel  @4: UnknownChannel;
        encrypted       @5: Data;
        # Nonce is a 96 bit counter, Additional authenticated data is channelId.
    }
}
