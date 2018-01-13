@0xa13c661ee4a5c8d7;

using import "common.capnp".CustomUInt128;
using import "common.capnp".CustomUInt256;
using import "common.capnp".CustomUInt512;
using import "common.capnp".Receipt;

# Token channel messages
# ----------------------

struct NeighborMoveToken {
        tokenChannelIndex @0: UInt8;
        transactions @1: List(Data);
        oldToken @2: CustomUInt256;
        randNonce @3: CustomUInt128;
}

struct NeighborInconsistencyError {
        tokenChannelIndex @0: UInt8;
        currentToken @1: CustomUInt256;
        balanceForReset @2: Int64;
}


# Token Transactions
# ------------------


struct SetRemoteMaxDebtTran {
        remoteMaxDebt @0: UInt64;
}


struct FundsRandNonceTran {
        fundsRandNonce @0: CustomUInt128;
}

struct LoadFundsTran {
        receipt @0: Receipt;
}


struct PlainContent {
        recipientTimestamp @0: CustomUInt128;
        senderTimestamp @1: CustomUInt128;
        senderCommPublicKey @2: CustomUInt256;
        destPort :union {
                funder @3: Void;
                indexerClient @4: Void;
                appManager @5: UInt32;
        }
        messageContent @6: Data;
}

struct EncMessage {
        senderCommPublicKey @0: CustomUInt256;
        keyDerivationNonce @1: CustomUInt256;
        destCommPublicKeyHash @2: UInt64;
        # First 8 bytes of sha512/256(destDhPublicKey)
        # Helps the destination find out which of his diffie-hellman public
        # keys was chosen.
        signature @3: CustomUInt512;
        # A signature over all the previous fields.
        encryptedContent @4: Data; 
        # Contains PlainContent, encrypted.
}

struct NeighborsRoute {
        publicKeys @0: List(CustomUInt256);
}

struct RequestSendMessageTran {
        requestId @0: CustomUInt128;
        route @1: NeighborsRoute;
        requestContent :union {
                commMeans @2: Void;
                encrypted @3: EncMessage;
        }
        maxResponseLength @4: UInt32;
        processingFeeProposal @5: UInt64;
        creditsPerByteProposal @6: UInt64;
}


struct ResponseSendMessageTran {
        requestId @0: CustomUInt128;
        randNonce @1: CustomUInt128;
        responseData @2: Data;
        signature @3: CustomUInt512;
        # Signature{key=recipientKey}(
        #   "MESSAGE_SUCCESS" ||
        #   requestId ||
        #   sha512/256(route) ||
        #   sha512/256(requestContent) ||
        #   maxResponseLength ||
        #   processingFeeProposal ||
        #   creditsPerByteProposal || 
        #   sha512/256(responseContent)
        #   randNonce)
}

struct FailedSendMessageTran {
        requestId @0: CustomUInt128;
        reportingPublicKey @1: CustomUInt256;
        randNonce @2: CustomUInt128;
        signature @3: CustomUInt512;
        # Signature{key=reportingNodePublicKey}(
        #   "MESSAGE_FAILURE" ||
        #   requestId ||
        #   sha512/256(route) ||
        #   sha512/256(requestContent) ||
        #   maxResponseLength ||
        #   processingFeeProposal ||
        #   creditsPerByteProposal || 
        #   randNonce)
}


struct ResetChannelTran {
        newBalance @0: UInt64;
}



# Requests sent directly to the Networker
# ---------------------------------------

# Node -> Node::Networker
# struct RequestNodeCommMeans {} # (Empty)

# Node::Networker -> Node
struct ResponseNodeCommMeans {
        commPublicKey @0: CustomUInt256;
        # diffie-hellman Public key used for communication.
        recentTimestamp @1: CustomUInt128;
        # A recent timestamp to be used for communication.
}

# Node -> Node::Networker
# struct RequestNodeNeighborsInfo {} # (Empty)

# Node::Networker -> Node
struct ResponseNodeNeighborsInfo {
        connectedNeighborsList @0: List(CustomUInt256);
        # A list of neighbors currently online.
}


