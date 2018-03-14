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
        randNonce @1: CustomUInt128;
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
        reportingPublicKeyIndex @1: UInt16;
        # Index of the reporting node in the route of the corresponding request.
        # The reporting public key could be any public key along the route,
        # except for the destination node. The destination node should not be
        # able to issue this message.
        randNonceSignatures @2: List(RandNonceSignature);
        # Signature{key=recipientKey}(
        #   "FUND_FAILURE" ||
        #   sha512/256(requestId || sha512/256(nodeIdPath) || mediatorPaymentProposal) ||
        #   invoiceId ||
        #   destinationPayment ||
        #   prev randNonceSignatures ||
        #   randNonce)
}


struct FriendTransaction {
        union {
                setState @0: SetStateTran;
                setRemoteMaxDebt @1: SetRemoteMaxDebtTran;
                requestSendFund @2: RequestSendFundTran;
                responseSendFund @3: ResponseSendFundTran;
                failedSendFund @4: FailedSendFundTran;
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
}

# Node::Funder -> Node
struct ResponseNodeFriendsInfo {
        connectedFriendsList @0: List(ConnectedFriend);
        # A list of friends currently online.
}
