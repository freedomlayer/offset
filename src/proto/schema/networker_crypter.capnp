@0xcaa25ded5bdbc81a;

# This file contains messages used for managing encrypted sessions between
# networkers. All messages are sent as requests or responses transactions in
# through a Networker Token Channel.

using import "common.capnp".CustomUInt128;
using import "common.capnp".CustomUInt256;
using import "common.capnp".CustomUInt512;


# 1. A -> B: RequestNonce         (randNonce1)
# 2. B -> A: ResponseNonce        (randNonce1, randNonceB, responseNonceB, Signature(B))
#
# 3. A -> B: ExchangeActive       (randNonceB, randNonceA, publicKeyA, publicKeyB,  dhPublicKeyA, keySaltA, Signature(A))
# 4. B -> A: ExchangePassive      (hashPrev, dhPublicKeyB, keySaltB, Signature(B))
# 5. A -> B: ChannelReady         (hashPrev, Signature(A))

##########################################

#[Request]
struct RequestNonce {
        randNonce1 @0: CustomUInt128;
}

#[Response]
struct ResponseNonce {
        randNonce1 @0: CustomUInt128;
        randNonceB @1: CustomUInt128;
        responseNonceB @2: CustomUInt128;
        signature @3: CustomUInt512;
}

###########################################

#[Request]
struct ExchangeActive {
        randNonceB @0: CustomUInt128;
        randNonceA @1: CustomUInt128;
        publicKeyA @2: CustomUInt256;
        publicKeyB @3: CustomUInt256;
        dhPublicKeyA @4: CustomUInt256;
        keySaltA @5: CustomUInt256;
        signature @6: CustomUInt512;
}

#[Response]
struct ExchangePassive {
        hashPrev @0: CustomUInt256;
        dhPublicKeyB @1: CustomUInt256;
        keySaltB @2: CustomUInt256;
        signature @3: CustomUInt512;
}

#[Request]
struct ChannelReady {
        hashPrev @0: CustomUInt256;
        signature @1: CustomUInt512;
}
# Note: This message must have an empty response.



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
                networker @1: Void;
                funder @2: Void;
                indexerClient @3: Void;
                appManager @4: UInt32;
        }
}

# All possible request messages:
struct RequestMessage {
        union {
                requestNonce @0: RequestNonce;
                exchangeActive @1: ExchangeActive;
                channelReady @2: ChannelReady;
                encrypted @3: Data;
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


# Requests sent directly to the Networker
# ---------------------------------------

# Node -> Node::Networker
# struct RequestNodeNeighborsInfo {} # (Empty)

struct ConnectedNeighbor {
        publicKey @0: CustomUInt256;
        requestBase @1: UInt32;
        requestMultiplier @2: UInt32;
}

# Node::Networker -> Node
struct ResponseNodeNeighborsInfo {
        connectedNeighborsList @0: List(ConnectedNeighbor);
        # A list of neighbors currently online.
}
