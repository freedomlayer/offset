@0xcaa25ded5bdbc81a;

# This file contains messages used for managing encrypted sessions between
# networkers. All messages are sent as requests or responses transactions in
# through a Networker Token Channel.

using import "common.capnp".CustomUInt128;
using import "common.capnp".CustomUInt256;
using import "common.capnp".CustomUInt512;


# 4-way handshake
# ---------------
# 1. A -> B: Handshake1 (randNonceA, publicKeyA) 
# 2. B -> A: Handshake2 (hashPrev, randNonceB, publicKeyB, dhPublicKeyB, keySaltB, Signature(B))
# 3. A -> B: Handshake3 (hashPrev, dhPublicKeyA, keySaltA, Signature(A))
# 4. B -> A: Handshake4 (hashPrev, Signature(B))


# Expects Handshake2 as response.
# [Request]
struct Handshake1 {
        randNonce1 @0: CustomUInt128;
        publicKey1 @1: CustomUInt256;
}

# [Response]
struct Handshake2 {
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
struct Handshake3 {
        hashPrev @0: CustomUInt256;
        dhPublicKey1 @1: CustomUInt256;
        keySalt1 @2: CustomUInt256;
        signature @3: CustomUInt512;
        # Requests are not signed by the request/response networker protocol,
        # therefore we need to have a signature here.
}

# Handshake4 is any signed response to Handshake3.


# The sender of this request indicates that it does not have the receiver end
# of the channel channelId. The response for this request is ignored.
# [Request]
struct UnknownChannel {
        channelId @0: CustomUInt128;
        randNonce @1: CustomUInt128;
        signature @2: CustomUInt512;
        # Signature(channelId || randNonce)
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
                handshake1 @0: Handshake1;
                handshake3 @1: Handshake3;
                unknownChannel @2: UnknownChannel;
                encrypted @3: Data;
        }
}

