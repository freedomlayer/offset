@0xbc544d9c10380608;

using import "common.capnp".CustomUInt128;
using import "common.capnp".CustomUInt256;
using import "common.capnp".CustomUInt512;

# 5-phase handshake process
# =========================
# 1. A -> B: RequestNonce     randNonceA
# 2. B -> A: ResponseNonce    (randNonceA, randNonceB, Signature(B)) [publicKeyB necessary?]
# 3. A -> B: ExchangeActive   (randNonceB, publicKeyA, publicKeyB dhPublicKeyA, keySaltA, Signature(A))
# 4. B -> A: ExchangePassive  (hashPrev, dhPublicKeyB, keySaltB, Signature(B))
# 5. A -> B: ChannelReady     (hashPrev, Signature(A))
#
# In the above scheme, A is called the initiator, and B is called the responder.
# The handshake session has a limited time to complete (measured from its creation). If it times
# out, then it is dropped.
# In all messages, the signature is over the whole message, except for the signature itself.
# randNonceB is used to validate that 

# Phases 1-2 exist to prevent a DoS by using a replay attack.
#
# Upon sending RequestNonce, the initiator creates a new handshake session.
# Implementation note: The initiator MUST keep nonce he sent for a while.
struct RequestNonce {
    # Plain random. Not taken from a constant size list.
    randNonce @0: CustomUInt128;
}

# Implementation note: The responder MUST NOT keep any thing at this phase, or else
# an attacker could consume all its memory by flooding the responder with
# RequestNonce messages.
struct ResponseNonce {
    receivedRandNonce @0: CustomUInt128;

    # This nonce is taken from a local constant size list of nonces
    # maintained by the responder. It is the newest nonce on this list.
    randNonce @1: CustomUInt128;
    signature @2: CustomUInt512;
}

# Upon receiving a ResponseNonce message, the initiator verifies the nonce first.
# Then, it generates a new DH public key and salt, and sends them to the responder.
#
# Implementation note: The initiator creates a new handshake session after it sends
# ExchangeActive message.
struct ExchangeActive {
    receivedRandNonce  @0: CustomUInt128;

    publicKeyInitiator @1: CustomUInt256;
    publicKeyResponder @2: CustomUInt256;
    dhPublicKey        @3: CustomUInt256;
    keySalt            @4: CustomUInt256;
    signature          @5: CustomUInt512;
}

# Upon receiving ExchangeActive message, the responder first checks whether an ongoing
# handshake session with the initiator already exists. If one exists, then the message
# is ignored.
# Then, the responder verifies the signature.
# Then, it verifies that the nonce indeed exists in its local list.
# Then, it generates a new DH key and salt and creates a new handshake session.
struct ExchangePassive {
    prevHash    @0: CustomUInt256;
    dhPublicKey @1: CustomUInt256;
    keySalt     @2: CustomUInt256;
    signature   @3: CustomUInt512;
}

# This message is used by the initiator to confirm that the responder received
# ExchangePassive, and then the 5-way handshake is complete.
struct ChannelReady {
    prevHash  @0: CustomUInt256;
    signature @1: CustomUInt512;
}

struct UnknownChannel {
    channelId @0: CustomUInt128;
    randNonce @1: CustomUInt128;
    signature @2: CustomUInt512;
}

# The data we encrypt. Contains random padding (of a variable length) together
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

