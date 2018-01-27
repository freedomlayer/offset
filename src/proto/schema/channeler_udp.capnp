@0xbc544d9c10380608;

using import "common.capnp".CustomUInt128;
using import "common.capnp".CustomUInt256;
using import "common.capnp".CustomUInt512;


# 4-way handshake
# ---------------
# 1. A -> B: Handshake1 (randNonceA, publicKeyA) 
# 2. B -> A: Handshake2 (hashPrev, randNonceB, publicKeyB, dhPublicKeyB, keySaltB, Signature(B))
# 3. A -> B: Handshake3 (hashPrev, dhPublicKeyA, keySaltA, Signature(A))
# 4. B -> A: Handshake4 (hashPrev, Signature(B))


struct Handshake1 {
        randNonce1 @0: CustomUInt128;
        publicKey1 @1: CustomUInt256;
}

struct Handshake2 {
        hashPrev @0: CustomUInt256;
        randNonce2 @1: CustomUInt128;
        publicKey2 @2: CustomUInt256;
        dhPublicKey2 @3: CustomUInt256;
        keySalt2 @4: CustomUInt256;
        signature @5: CustomUInt512;
        # Signature is applied for all the previous fields
        # Signature("Handshake2" || fields ...)
}

struct Handshake3 {
        hashPrev @0: CustomUInt256;
        dhPublicKey1 @1: CustomUInt256;
        keySalt1 @2: CustomUInt256;
        signature @3: CustomUInt512;
        # Signature is applied for all the previous fields
        # Signature("Handshake3" || fields ...)
}

struct Handshake4 {
        hashPrev @0: CustomUInt256;
        signature @1: CustomUInt512;
        # Signature is applied for all the previous fields
        # Signature("Handshake4" || fields ...)
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
                handshake1 @0: Handshake1;
                handshake2 @1: Handshake2;
                handshake3 @2: Handshake3;
                handshake4 @3: Handshake4;
                unknownChannel @4: UnknownChannel;
                encrypted @5: Data;
                # Nonce is a 96 bit counter, Additional authenticated data is
                # channelId.
        }
}
