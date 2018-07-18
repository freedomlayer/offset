@0xe7603b9ac00e2251;

using import "common.capnp".CustomUInt128;
using import "common.capnp".CustomUInt256;
using import "common.capnp".CustomUInt512;
using import "common.capnp".Ratio128;
using import "common.capnp".RandNonceSignature;


# Token channel messages
# ----------------------

struct FriendMoveToken {
        operations @0: List(FriendOperation);
        oldToken @1: CustomUInt256;
        randNonce @2: CustomUInt128;
}


struct FriendInconsistencyError {
        currentToken @0: CustomUInt256;
        balanceForReset @1: CustomUInt128;
        # Note that this is actually a signed number (Highest bit is the sign
        # bit, Two's complement method). TODO: Should we have a separate type,
        # like CustomInt128?
}


# Token Operations
# ------------------

struct EnableRequestsOp {
        base @0: UInt64;
        multiplier @1: UInt64;
        # The sender of this message declares that
        # Sending x bytes to the remote side costs `base + x * multiplier`
        # credits.
}
# This message may be sent more than once, to update the values of base and multiplier.


# struct DisableRequests {
# }

# Set the maximum possible debt for the remote party.
# Note: It is not possible to set a maximum debt smaller than the current debt
# This will cause an inconsistency.
struct SetRemoteMaxDebtOp {
        remoteMaxDebt @0: UInt64;
}

struct FunderSendPrice {
        base @0: UInt64;
        multiplier @1: UInt64;
}

struct FriendRouteLink {
        nodePublicKey @0: CustomUInt256;
        # Public key of current node
        requestProposal @1: FunderSendPrice;
        # Payment proposal for sending data to the next node.
        responseProposal @2: FunderSendPrice;
        # Payment proposal for sending data to the previous node.
}


struct FriendsRoute {
        sourcePublicKey @0: CustomUInt256;
        # Public key for the message originator.
        sourceResponseProposal @1: FunderSendPrice;
        # Payment proposal for sending data from source forward.
        # This amount is not used in the calculation of costs for sending the
        # request, but can be used by the receiver to produce a route back to
        # the origin of the request.
        routeLinks @2: List(FriendRouteLink);
        # A chain of all intermediate nodes.
        destPublicKey @3: CustomUInt256;
        # Public key for the message destination.
        destResponseProposal @4: FunderSendPrice;
        # Payment proposal for sending data from dest backwards.
}

struct FriendFreezeLink {
        sharedCredits @0: UInt64;
        # Credits shared for freezing through previous edge.
        usableRatio @1: Ratio128;
        # Ratio of credits that can be used for freezing from the previous
        # edge. Ratio might only be an approximation to real value, if the real
        # value can not be represented as a u128/u128.
}


struct RequestSendFundsOp { 
        requestId @0: CustomUInt128;
        destPayment @1: CustomUInt128;
        route @2: FriendsRoute;
        invoiceId @3: CustomUInt256;
        freezeLinks @4: List(FriendFreezeLink);
        # Variable amount of freezing links. This is used for protection
        # against DoS of credit freezing by have exponential decay of available
        # credits freezing according to derived trust.
        # This part should not be signed in the Response message.
}

struct ResponseSendFundsOp {
        requestId @0: CustomUInt128;
        randNonce @1: CustomUInt128;
        signature @2: CustomUInt512;
        # Signature{key=recipientKey}(
        #   sha512/256("FUND_SUCCESS") ||
        #   sha512/256(requestId || sha512/256(route) || randNonce) ||
        #   invoiceId ||
        #   destPayment
        # )
        #
        # Note that the signature contains an inner blob (requestId || ...).
        # This is done to make the size of the receipt shorter.
        # See also the Receipt structure.
}

struct FailureSendFundsOp {
        requestId @0: CustomUInt128;
        reportingPublicKey @1: CustomUInt256;
        # Index of the reporting node in the route of the corresponding request.
        # The reporting node cannot be the destination node.
        randNonceSignatures @2: List(RandNonceSignature);
        # Contains a signature for every node in the route, from the reporting
        # node, until the current node.
        # Signature{key=recipientKey}(
        #   sha512/256("FUND_FAILURE") ||
        #   requestId ||
        #   destPayment ||
        #   sha512/256(route) || 
        #   invoiceId ||
        #   reportingPublicKey ||
        #   prev randNonceSignatures ||
        #   randNonce
        # )
}


struct FriendOperation {
        union {
                enableRequests @0: EnableRequestsOp;
                disableRequests @1: Void;
                setRemoteMaxDebt @2: SetRemoteMaxDebtOp;
                requestSendFunds @3: RequestSendFundsOp;
                responseSendFunds @4: ResponseSendFundsOp;
                failedSendFunds @5: FailureSendFundsOp;
        }
}


# Requests sent directly to the Funder
# ------------------------------------

# Node -> Node::Funder
# struct RequestNodeFriendsInfo {} # (Empty)

struct ConnectedFriend {
        sendCapacity @0: CustomUInt128;
        recvCapacity @1: CustomUInt128;
        publicKey @2: CustomUInt256;
        requestBase @3: UInt64;
        requestMultiplier @4: UInt64;
}

# Node::Funder -> Node
struct ResponseNodeFriendsInfo {
        connectedFriendsList @0: List(ConnectedFriend);
        # A list of friends currently online.
}
