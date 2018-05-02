@0xa13c661ee4a5c8d7;

using import "common.capnp".CustomUInt128;
using import "common.capnp".CustomUInt256;
using import "common.capnp".CustomUInt512;
using import "common.capnp".Receipt;
using import "common.capnp".RandNonceSignature;

# Token channel messages
# ----------------------

struct NeighborMoveToken {
        tokenChannelIndex @0: UInt8;
        union {
                transactions @1: List(NeighborTransaction);
                resetChannel @2: Int64;
        }
        oldToken @3: CustomUInt256;
        randNonce @4: CustomUInt128;
}

struct NeighborInconsistencyError {
        tokenChannelIndex @0: UInt8;
        currentToken @1: CustomUInt256;
        balanceForReset @2: Int64;
}


# Token Transactions
# ------------------

struct EnableRequests {
        base @0: UInt32;
        multiplier @1: UInt32;
        # The sender of this message declares that
        # Sending x bytes to the remote side costs `base + x * multiplier`
        # credits.
}
# This message may be sent more than once, to update the values of base and multiplier.


# struct DisableRequests {
# }

struct SetRemoteMaxDebtTran {
        remoteMaxDebt @0: UInt64;
}


struct SetInvoiceIdTran {
        invoiceId @0: CustomUInt256;
}

struct LoadFundsTran {
        receipt @0: Receipt;
}


struct NeighborRouteLink {
        nodePublicKey @0: CustomUInt256;
        # Public key of current node
        requestBaseProposal @1: UInt32;
        # request base pricing for the current node
        requestMultiplierProposal @2: UInt32;
        # request multiplier pricing for the current node.
        responseBaseProposal @3: UInt32;
        # response base pricing for the next node.
        responseMultiplierProposal @4: UInt32;
        # response multiplier pricing for the next node.
}


struct NeighborsRoute {
        sourcePublicKey @0: CustomUInt256;
        # Public key for the message originator.
        routeLinks @1: List(NeighborRouteLink);
        # A chain of all intermediate nodes.
        destinationPublicKey @2: CustomUInt256;
        # Public key for the message destination.
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
        reportingPublicKeyIndex @1: UInt16;
        # Index on the route of the public key reporting this failure message.
        # The destination node should not be able to issue this message.
        randNonceSignatures @2: List(RandNonceSignature);
        # Contains a signature for every node in the route, from the reporting
        # node, until the current node.
        # Signature{key=reportingNodePublicKey}(
        #   "REQUEST_FAILURE" ||
        #   requestId ||
        #   sha512/256(route) ||
        #   sha512/256(requestContent) ||
        #   maxResponseLength ||
        #   processingFeeProposal ||
        #   creditsPerByteProposal || 
        #   prev randNonceSignatures ||
        #   randNonce)
}


struct NeighborTransaction {
        union {
                enableRequests @0: EnableRequests;
                disableRequests @1: Void;
                setRemoteMaxDebt @2: SetRemoteMaxDebtTran;
                setInvoiceIdTran @3: SetInvoiceIdTran;
                loadFunds @4: LoadFundsTran;
                requestSendMessage @5: RequestSendMessageTran;
                responseSendMessage @6: ResponseSendMessageTran;
                failedSendMessage @7: FailedSendMessageTran;
        }
}


