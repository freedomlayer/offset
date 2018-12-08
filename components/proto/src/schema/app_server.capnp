@0xcd5fc5928aa22c39;

using import "funder.capnp".FriendsRoute;
using import "funder.capnp".FunderSendPrice;
using import "common.capnp".Receipt;
using import "common.capnp".Uid;
using import "common.capnp".InvoiceId;
using import "common.capnp".CustomUInt128;
using import "common.capnp".CustomInt128;
using import "common.capnp".PublicKey;
using import "common.capnp".Hash;
using import "common.capnp".Signature;
using import "common.capnp".RandNonce;


# Interface between AppServer and an Application
################################################

struct RequestSendFunds {
        paymentId @0: Uid;
        destPayment @1: CustomUInt128;
        route @2: FriendsRoute;
        invoiceId @3: InvoiceId;
}

struct SuccessSendFunds {
        receipt @0: Receipt;
} 

struct FailureSendFunds {
        reportingPublicKey @0: PublicKey;
}

struct ResponseSendFunds {
        paymentId @0: CustomUInt128;
        response: union {
                success @1: SuccessSendFunds;
                failure @2: FailureSendFunds;
        }
}

struct ReceiptAck {
        paymentId @0: Uid;
        receiptHash @1: Hash;
}


# IP address and port
# TODO: Should add IPV6?
struct SocketAddress {
        address @0: UInt32;
        port @1: UInt16;
}

# Authenticated address of a Relay (Includes relay's public key)
struct RelayAddress {
        publicKey @0: PublicKey;
        socketAddress @1: SocketAddress;
}


# App -> AppServer
struct SetAddress {
    union {
        address @0: RelayAddress;
        empty @1: Void;
    }
}

# Application -> AppServer
struct AddFriend {
        friendPublicKey @0: PublicKey;
        address @1: RelayAddress;
        name @2: Text;
        balance @3: CustomInt128;
}

# Application -> AppServer
struct SetFriendInfo {
        friendPublicKey @0: PublicKey;
        address @1: RelayAddress;
        name @2: Text;
}

# Application -> AppServer
struct RemoveFriend {
        friendPublicKey @0: PublicKey;
}

# Application -> AppServer
struct OpenFriend {
        friendPublicKey @0: PublicKey;
}

# Application -> AppServer
struct CloseFriend {
        friendPublicKey @0: PublicKey;
}

# Application -> AppServer
struct EnableFriend {
        friendPublicKey @0: PublicKey;
}

# Application -> AppServer
struct DisableFriend {
        friendPublicKey @0: PublicKey;
}

# Application -> AppServer
struct SetFriendRemoteMaxDebt {
        friendPublicKey @0: PublicKey;
        remoteMaxDebt @1: CustomUInt128;
}

# Application -> AppServer
struct ResetFriendChannel {
        friendPublicKey @0: PublicKey;
        currentToken @1: Signature;
}

# App -> AppServer
struct RequestDelegate {
        appRandNonce @0: RandNonce;
}

# AppServer -> App
struct ResponseDelegate {
        appPublicKey @0: PublicKey;
        appRandNonce @1: RandNonce;
        serverRandNonce @2: RandNonce;
        signature @3: Signature;
        # sha512/256(sha512/256("DELEGATE") ||
        #               appPublicKey ||
        #               appRandNonce ||
        #               serverRandNonce)

}



##################################################
## Report related structs
##################################################

struct MoveTokenHashed {
        operationsHash @0: Hash;
        oldToken @1: Signature;
        inconsistencyCounter @2: UInt64;
        moveTokenCounter @3: CustomUInt128;
        balance @4: CustomInt128;
        localPendingDebt @5: CustomUInt128;
        remotePendingDebt @6: CustomUInt128;
        randNonce @7: RandNonce;
        newToken @8: Signature;
}


enum FriendStatus {
        disabled @0;
        enabled @1;
}

enum RequestsStatus {
        closed @0;
        open @1;
}

enum FriendLivenessReport {
        offline @0;
        online @1;
}

enum DirectionReport {
        incoming @0;
        outgoing @1;
}

struct McRequestsStatus {
        local @0: RequestsStatus;
        remote @1: RequestsStatus;
}

struct TcReport {
        direction @0: DirectionReport;
        balance @1: CustomInt128;
        requestsStatus @2: McRequestsStatus;
        numLocalPendingRequests @3: UInt64;
        numRemotePendingRequests @4: UInt64;
}

struct ChannelInconsistentReport {
        localResetTermsBalance @0: CustomInt128;
        optRocalResetTermsBalance: union {
                remoteResetTerms @1: CustomInt128;
                empty @2: Void;
        }
}


struct ChannelStatusReport {
        union {
                inconsistent @0: ChannelInconsistentReport;
                consistenet @1: TcReport;
        }
}

struct OptLastIncomingMoveToken {
        union {
                moveTokenHashed @0: MoveTokenHashed;
                empty @1: Void;
        }
}


struct FriendReport {
        address @0: RelayAddress;
        name @1: Text;
        optLastIncomingMoveToken @2: OptLastIncomingMoveToken;
        liveness @3: FriendLivenessReport;
        channelStatus @4: ChannelStatusReport;
        wantedRemoteMaxDebt @5: CustomUInt128;
        wantedLocalRequestsStatus @6: RequestsStatus;
        numPendingRequests @7: UInt64;
        numPendingResponses @8: UInt64;
        status @9: FriendStatus;
        numPendingUserRequests @10: UInt64;
}

struct Report {
        localPublicKey @0: PublicKey;
        optAddress: union {
                address @1: RelayAddress;
                empty @2: Void;
        }
        friends @3: List(FriendReport);
        numReadyReceipts @4: UInt64;
}

struct SetAddressReport {
    union {
        address @0: RelayAddress;
        empty @1: Void;
    }
}

struct AddFriendReport {
        friendPublicKey @0: PublicKey;
        address @1: RelayAddress;
        name @2: Text;
        balance @3: CustomInt128;
        optLastIncomingMoveToken @4: OptLastIncomingMoveToken;
        channelStatus @5: ChannelStatusReport;
}

struct RelayAddressName {
        address @0: RelayAddress;
        name @1: Text;
}

struct FriendReportMutation {
        union {
                setFriendInfo @0: RelayAddressName;
                setChannelStatus @1: ChannelStatusReport;
                setWantedRemoteMaxDebt @2: CustomUInt128;
                setWantedLocalRequestsStatus @3: RequestsStatus;
                setNumPendingRequests @4: UInt64;
                setNumPendingResponses @5: UInt64;
                setFriendStatus @6: FriendStatus;
                setNumPendingUserRequests @7: UInt64;
                setOptLastIncomingMoveToken @8: OptLastIncomingMoveToken;
                setLiveness @9: FriendLivenessReport;
        }
}

struct PkFriendReportMutation {
        friendPublicKey @0: PublicKey;
        friendReportMutation @1: FriendReportMutation;
}

struct ReportMutation {
        union {
                setAddress @0: SetAddressReport;
                addFriend @1: AddFriendReport;
                removeFriend @2: PublicKey;
                pkFriendReportMutation @3: PkFriendReportMutation;
                setNumReadyReceipts @4: UInt64;
        }
}

#####################################################################

struct AppServerToApp {
    union {
        # Funds
        responseSendFunds @0: ResponseSendFunds;

        # Reports about current state:
        report @1: Report;
        reportMutations @2: List(ReportMutation);

        # Response for delegate request:
        responseDelegate @3: ResponseDelegate;

    }
}


struct AppToAppServer {
    union {
        setAddress @0: SetAddress;

        # Sending Funds:
        requestSendFunds @1: RequestSendFunds;
        receiptAck @2: ReceiptAck;

        # Friends management
        addFriend @3: AddFriend;
        setFriendInfo @4: SetFriendInfo;
        removeFriend @5: RemoveFriend;
        enableFriend @6: EnableFriend;
        disableFriend @7: DisableFriend;
        openFriend @8: OpenFriend;
        closeFriend @9: CloseFriend;
        setFriendRemoteMaxDebt @10: SetFriendRemoteMaxDebt;
        resetFriendChannel @11: ResetFriendChannel;

        # Delegation:
        requestDelegate @12: RequestDelegate;
    }
}

