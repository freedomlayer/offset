@0xa13c661ee4a5c8d7;

using import "common.capnp".CustomUInt128;
using import "common.capnp".CustomUInt256;
using import "common.capnp".CustomUInt512;
using import "common.capnp".Rational64;
using import "common.capnp".Receipt;
using import "common.capnp".RandNonceSignature;

# Token channel messages
# ----------------------

struct NeighborMoveToken {
        tokenChannelIndex @0: UInt8;
        union {
                transactions @1: List(NeighborOperation);
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


# Token Operations
# ------------------

struct EnableRequestsOp {
        base @0: UInt32;
        multiplier @1: UInt32;
        # The sender of this message declares that
        # Sending x bytes to the remote side costs `base + x * multiplier`
        # credits.
}
# This message may be sent more than once, to update the values of base and multiplier.


# struct DisableRequestsOp {
# }

struct SetRemoteMaxDebtOp {
        remoteMaxDebt @0: UInt64;
}


# TODO:
# Find out relation between freezing amounts
# and remoteMaxDebt. Possibly put all of those in one message (SetRemoteMaxDebtOp).
#       struct SetRemoteMaxFreezeOp {
#               maxFreezes @0: List(MaxFreezeItem);
#               struct MaxFreezeItem {
#                       publicKey @0: CustomUInt256;
#                       maxFreeze @1: UInt64;
#               }
#       }


struct SetInvoiceIdOp {
        invoiceId @0: CustomUInt256;
}

struct LoadFundsOp {
        receipt @0: Receipt;
}

struct NetworkerSendPrice {
        base @0: UInt32;
        multiplier @1: UInt32;
}


struct NeighborRouteLink {
        nodePublicKey @0: CustomUInt256;
        # Public key of current node
        requestProposal @1: NetworkerSendPrice;
        # Payment proposal for sending data to the next node.
        responseProposal @2: NetworkerSendPrice;
        # Payment proposal for sendingdata to the previous node.
}


struct NeighborsRoute {
        sourcePublicKey @0: CustomUInt256;
        # Public key for the message originator.
        sourceResponseProposal @1: NetworkerSendPrice;
        # Payment proposal for sending data from the source forward.  Note that
        # this information is not required for calculation of costs for the
        # current request, however it can be used by the receiver of the
        # request message to construct a route quickly for sending a request back.
        routeLinks @2: List(NeighborRouteLink);
        # A chain of all intermediate nodes.
        destPublicKey @3: CustomUInt256;
        # Public key for the message destination.
        destResponseProposal @4: NetworkerSendPrice;
        # Payment proposal for sending data from dest backwards
}

struct NeighborFreezeLink {
        sharedCredits @0: UInt64;
        # Credits shared for freezing through previous edge.
        usableRatio @1: Rational64;
        # Ratio of credits that can be used for freezing from the previous
        # edge. Ratio might only be an approximation to real value, if the real
        # value can not be represented as a u64/u64.
}


struct RequestSendMessageOp {
        requestId @0: CustomUInt128;
        maxResponseLength @1: UInt32;
        processingFeeProposal @2: UInt64;
        route @3: NeighborsRoute;
        requestContent @4: Data;
        freezeLinks @5: List(NeighborFreezeLink);
        # Variable amount of freezing links. This is used for protection
        # against DoS of credit freezing by have exponential decay of available
        # credits freezing according to derived trust.
        # This part should not be signed in the Response message.
}


struct ResponseSendMessageOp {
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
        #   maxResponseLength ||
        #   processingFeeProposal ||
        #   sha512/256(route) ||
        #   sha512/256(requestContent) ||
        #   processingFeeCollected ||
        #   sha512/256(responseContent) ||
        #   randNonce)
}



struct FailedSendMessageOp {
        requestId @0: CustomUInt128;
        reportingPublicKey @1: CustomUInt256;
        # Index on the route of the public key reporting this failure message.
        # The destination node should not be able to issue this message.
        randNonceSignatures @2: List(RandNonceSignature);
        # Contains a signature for every node in the route, from the reporting
        # node, until the current node.
        # Signature{key=reportingNodePublicKey}(
        #   "REQUEST_FAILURE" ||
        #   requestId ||
        #   maxResponseLength ||
        #   processingFeeProposal ||
        #   sha512/256(route) ||
        #   sha512/256(requestContent) ||
        #   processingFeeProposal ||
        #   reportingPublicKey ||
        #   prev randNonceSignatures ||
        #   randNonce)
}


struct NeighborOperation {
        union {
                enableRequests @0: EnableRequestsOp;
                disableRequests @1: Void;
                setRemoteMaxDebt @2: SetRemoteMaxDebtOp;
                setInvoiceId @3: SetInvoiceIdOp;
                loadFunds @4: LoadFundsOp;
                requestSendMessage @5: RequestSendMessageOp;
                responseSendMessage @6: ResponseSendMessageOp;
                failedSendMessage @7: FailedSendMessageOp;
        }
}


