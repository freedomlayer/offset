@0xe7603b9ac00e2251;

using import "common.capnp".CustomUInt128;
using import "common.capnp".CustomUInt256;
using import "common.capnp".CustomUInt512;


# Token channel messages
# ----------------------

struct FriendMoveToken {
        transactions @0: List(Data);
        oldToken @1: CustomUInt256;
        randNonce @2: CustomUInt256;
}


struct FriendInconsistencyError {
        currentToken @0: CustomUInt256;
        balanceForReset @1: CustomUInt128;
}


# Token Transactions
# ------------------


enum RequestsState {
        enabled @0;
        disabled @1;
}

# Set requests state for the remote party.
# If the state is enabled, requests may be opened from the remote party.
# If the state is disabled, requests may not be opened from the remote party.
struct SetStateTran {
        newState @0: RequestsState;
}

# Set the maximum possible debt for the remote party.
# Note: It is not possible to set a maximum debt smaller than the current debt
# This will cause an inconsistency.
struct SetRemoteMaxDebtTran {
        remoteMaxDebt @0: UInt64;
}

struct FriendsRoute {
        publicKeys @0: List(CustomUInt256);
}

struct RequestSendFundTran { 
        requestId @0: CustomUInt128;
        route @1: FriendsRoute;
        mediatorPaymentProposal @2: UInt64;
        invoiceId @3: CustomUInt256;
        destinationPayment @4: CustomUInt128;
}

struct ResponseSendFundTran {
        requestId @0: CustomUInt128;
        randNonce @1: CustomUInt256;
        signature @2: CustomUInt512;
        # Signature{key=recipientKey}(
        #   "FUND_SUCCESS" ||
        #   sha512/256(requestId || sha512/256(nodeIdPath) || mediatorPaymentProposal) ||
        #   invoiceId ||
        #   destinationPayment ||
        #   randNonce)
}

struct FailedSendFundTran {
        requestId @0: CustomUInt128;
        reportingNodePublicKey @1: CustomUInt256;
        randNonce @2: CustomUInt256;
        signature @3: CustomUInt512;
        # Signature{key=recipientKey}(
        #   "FUND_FAILURE" ||
        #   sha512/256(requestId || sha512/256(nodeIdPath) || mediatorPaymentProposal) ||
        #   invoiceId ||
        #   destinationPayment ||
        #   randNonce)
}


struct ResetChannelTran {
        newBalance @0: UInt64;
}


# Requests sent directly to the Funder
# ------------------------------------

# Node -> Node::Funder
# struct RequestNodeFriendsInfo {} # (Empty)

struct ConnectedFriend {
        sendCapacity @0: CustomUInt128;
        recvCapacity @1: CustomUInt128;
        publicKey @2: CustomUInt256;
}

# Node::Funder -> Node
struct ResponseNodeFriendsInfo {
        connectedFriendsList @0: List(ConnectedFriend);
        # A list of friends currently online.
}
