@0xcaa25ded5bdbc81a;

# This file contains messages used for managing encrypted sessions between
# networkers. All messages are sent as requests or responses transactions in
# through a Networker Token Channel.

using import "common.capnp".CustomUInt128;
using import "common.capnp".CustomUInt256;
using import "common.capnp".CustomUInt512;

# 4-way handshake
# ---------------
# 1. A -> B: InitChannel      (randNonceA, publicKeyA)
# 2. B -> A: ExchangePassive  (hashPrev, randNonceB, publicKeyB, dhPublicKeyB, keySaltB, Signature(B))
# 3. A -> B: ExchangeActive   (hashPrev, dhPublicKeyA, keySaltA, Signature(A))
# 4. B -> A: ChannelReady     (hashPrev, Signature(B))


# Expects ExchangePassive as response.
# [Request]
struct InitChannel {
        randNonce1 @0: CustomUInt128;
        publicKey1 @1: CustomUInt256;
}

# [Response]
struct ExchangePassive {
        randNonce2 @0: CustomUInt128;
        publicKey2 @1: CustomUInt256;
        dhPublicKey2 @2: CustomUInt256;
        keySalt2 @3: CustomUInt256;
        # We don't have a signature here because responses are signed by the
        # request/response networker protocol.
}

# The sender of this request considers the connection to be completed when a
# signed response is received for this message. The receiver of this message
# considers the connection to be completed.
# [Request]
struct ExchangeActive {
        hashPrev @0: CustomUInt256;
        dhPublicKey1 @1: CustomUInt256;
        keySalt1 @2: CustomUInt256;
        signature @3: CustomUInt512;
        # Requests are not signed by the request/response networker protocol,
        # therefore we need to have a signature here.
}

# ChannelReady is any signed response to ExchangeActive.


# The sender of this response indicates that it does not have the receiver end
# of the channel channelId. This message can only be sent as a response to an
# encrypted request.
# [Response]
struct UnknownChannel {
        channelId @0: CustomUInt128;
        randNonce @1: CustomUInt128;
        # Signature is not required because responses are signed by the outer
        # protocol.
}

# The data we encrypt. Contains random padding (Of variable length) together
# with actual data. This struct is used for any data we encrypt.
struct Plain {
   randPadding @0: Data;
   # Random padding: Makes traffic analysis harder.
   content @1: Data;
}


# This structure is used as the content field inside the Plain struct.
struct PlainRequest {
        plainContent @0: Data;
        destPort :union {
                funder @1: Void;
                indexerClient @2: Void;
                appManager @3: UInt32;
        }
}

# All possible request messages:
struct RequestMessage {
        union {
                initChannel @0: InitChannel;
                exchangeActive @1: ExchangeActive;
                encrypted @2: Data;
        }
}

# A response for RequestMesasge.encrypted request.
struct ResponseToEncryptedRequest {
        # TODO: What is the real size of a union in capnp? This is important,
        # because we need to know how much space should we allocate in credit
        # for the response.
        # Is it possible to have an empty response here as a default empty
        # encrypted response, for saving in space?
        union {
                encrypted @0: Data;
                unknownChannel @1: UnknownChannel;
        }
}

