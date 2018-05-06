@0xe7603b9ac00e2251;

using import "common.capnp".CustomUInt128;
using import "common.capnp".CustomUInt256;
using import "common.capnp".CustomUInt512;
using import "common.capnp".RandNonceSignature;


# Token channel messages
# ----------------------

struct FriendMoveToken {
        union {
                transactions @0: List(FriendTransaction);
                resetChannel @1: CustomUInt128;
                # Note that this is actually a signed number (Highest bit is the sign
                # bit, Two's complement method). TODO: Should we have a separate type,
                # like CustomInt128?
        }
        oldToken @2: CustomUInt256;
        randNonce @3: CustomUInt128;
}


struct FriendInconsistencyError {
        currentToken @0: CustomUInt256;
        balanceForReset @1: CustomUInt128;
        # Note that this is actually a signed number (Highest bit is the sign
        # bit, Two's complement method). TODO: Should we have a separate type,
        # like CustomInt128?
}


# Token Transactions
# ------------------

struct EnableRequests {
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
struct SetRemoteMaxDebtTran {
        remoteMaxDebt @0: UInt64;
}

struct FriendRouteLink {
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


struct FriendsRoute {
        sourcePublicKey @0: CustomUInt256;
        # Public key for the message originator.
        routeLinks @1: List(FriendRouteLink);
        # A chain of all intermediate nodes.
        destinationPublicKey @2: CustomUInt256;
        # Public key for the message destination.
}

struct RequestSendFundTran { 
        requestId @0: CustomUInt128;
        route @1: FriendsRoute;
        invoiceId @2: CustomUInt256;
        destinationPayment @3: CustomUInt128;
}

struct ResponseSendFundTran {
        requestId @0: CustomUInt128;
        randNonce @1: CustomUInt128;
        signature @2: CustomUInt512;
        # Signature{key=recipientKey}(
        #   "FUND_SUCCESS" ||
        #   sha512/256(requestId || sha512/256(nodeIdPath) || 
        #   invoiceId ||
        #   destinationPayment ||
        #   randNonce)
}

struct FailedSendFundTran {
        requestId @0: CustomUInt128;
        reportingPublicKeyIndex @1: UInt16;
        # Index of the reporting node in the route of the corresponding request.
        # The reporting npde cannot be the destination node.
        randNonceSignatures @2: List(RandNonceSignature);
        # Contains a signature for every node in the route, from the reporting
        # node, until the current node.
        # Signature{key=recipientKey}(
        #   "FUND_FAILURE" ||
        #   sha512/256(requestId || sha512/256(nodeIdPath) || 
        #   invoiceId ||
        #   destinationPayment ||
        #   prev randNonceSignatures ||
        #   randNonce)
}


struct FriendTransaction {
        union {
                enableRequests @0: EnableRequests;
                disableRequests @1: Void;
                setRemoteMaxDebt @2: SetRemoteMaxDebtTran;
                requestSendFund @3: RequestSendFundTran;
                responseSendFund @4: ResponseSendFundTran;
                failedSendFund @5: FailedSendFundTran;
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
