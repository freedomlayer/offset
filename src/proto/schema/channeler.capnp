@0xbc544d9c10380608;

using import "common.capnp".CustomUInt128;
using import "common.capnp".CustomUInt256;
using import "common.capnp".CustomUInt512;

# 5-phase handshake process
# =========================
# 1. A -> B: RequestNonce     randNonce1
# 2. B -> A: ResponseNonce    (randNonce1, randNonceB, Signature(B)) [publicKeyB necessary?]
# 3. A -> B: ExchangeActive   (randNonceB, randNonceA, publicKeyA, dhPublicKeyA, keySaltA, Signature(A))
# 4. B -> A: ExchangePassive  (hashPrev, dhPublicKeyB, keySaltB, Signature(B))
# 5. A -> B: ChannelReady     (hashPrev, Signature(A))

# At the first phase, initiator send a RequestNonce message to the responder
# for requesting a nonce, which will be used in performing DH key exchange.
#
# Implement note: The initiator MUST keep nonce he sent for a while.
struct RequestNonce {
    randNonce @0: CustomUInt128;
}

# After receiving RequestNonce message, responder generate a new nonce, then
# sign (receivedRandNonce || generatedNonce), then send ResponseNonce back to
# initiator.
#
# Implement note: The responder MUST NOT keep any thing at this phase.
struct ResponseNonce {
    receivedRandNonce @0: CustomUInt128;

    randNonce @1: CustomUInt128;
    signature @2: CustomUInt512;
}

# After receiving a ResponseNonce message, initiator verify the nonce first.
# Then generates a new nonce, DH public key and salt, send these information
# together with its public key to responder, these information MUST be signed.
#
# Implement note: Initiator create a new handshake session after it sending
# ExchangeActive message.
struct ExchangeActive {
    receivedRandNonce   @0: CustomUInt128;

    randNonce   @1: CustomUInt128;
    publicKey   @2: CustomUInt256;
    dhPublicKey @3: CustomUInt256;
    keySalt     @4: CustomUInt256;
    signature   @5: CustomUInt512;
}

# After receiving ExchangeActive message, responder verify the signature first.
# Then create a new handshake session. As response, it generate DH public key,
# salt, then send to the initiator, these information MUST be signed.
struct ExchangePassive {
    prevHash    @0: CustomUInt256;
    dhPublicKey @1: CustomUInt256;
    keySalt     @2: CustomUInt256;
    signature   @3: CustomUInt512;
}

# This message is used by initiator to confirm that the responder received
# ExchangePassive, and the 5-way handshake done.
struct ChannelReady {
    prevHash  @0: CustomUInt256;
    signature @1: CustomUInt512;
}

struct UnknownChannel {
    channelId @0: CustomUInt128;
    randNonce @1: CustomUInt128;
    signature @2: CustomUInt512;
}

# The data we encrypt. Contains random padding (Of variable length) together
# with actual data. This structure is used for any data we encrypt.
struct Plain {
    randPadding  @0: Data;
    content :union {
        application @1: Data;
        keepAlive   @2: Void;
    }
}

struct ChannelerMessage {
    union {
        requestNonce    @0: RequestNonce;
        responseNonce   @1: ResponseNonce;
        exchangeActive  @2: ExchangeActive;
        exchangePassive @3: ExchangePassive;
        channelReady    @4: ChannelReady;
        unknownChannel  @5: UnknownChannel;
        encrypted       @6: Data;
    }
}

