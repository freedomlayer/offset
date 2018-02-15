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



struct NeighborsRoute {
        publicKeys @0: List(CustomUInt256);
}


struct RequestSendMessageTran {
        requestId @0: CustomUInt128;
        route @1: NeighborsRoute;
        requestContent @2: Data;
        maxResponseLength @3: UInt32;
        processingFeeProposal @4: UInt64;
        creditsPerByteProposal @5: UInt64;
}


struct ResponseSendMessageTran {
        requestId @0: CustomUInt128;
        randNonce @1: CustomUInt128;
        processingFeeCollected @2: UInt64;
        # The amount of credit actually collected from the proposed
        # processingFee. This value is at most request.processingFeeProposal.
        responseContent @3: Data;
        signature @4: CustomUInt512;
        # Signature{key=recipientKey}(
        #   "REQUEST_SUCCESS" ||
        #   requestId ||
        #   sha512/256(route) ||
        #   sha512/256(requestContent) ||
        #   maxResponseLength ||
        #   processingFeeProposal ||
        #   creditsPerByteProposal || 
        #   processingFeeCollected ||
        #   sha512/256(responseContent) ||
        #   randNonce)
}

struct FailedSendMessageTran {
        requestId @0: CustomUInt128;
        reportingPublicKey @1: CustomUInt256;
        # The reporting public key could be any public key along the route,
        # except for the destination node. The destination node should not be
        # able to issue this message.
        randNonce @2: CustomUInt128;
        signature @3: CustomUInt512;
        # Signature{key=reportingNodePublicKey}(
        #   "REQUEST_FAILURE" ||
        #   requestId ||
        #   sha512/256(route) ||
        #   sha512/256(requestContent) ||
        #   maxResponseLength ||
        #   processingFeeProposal ||
        #   creditsPerByteProposal || 
        #   randNonce)
}


struct ResetChannelTran {
        newBalance @0: Int64;
}


