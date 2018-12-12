@0xcd5fc5928aa22c39;

using import "funder.capnp".FriendsRoute;
using import "common.capnp".Uid;
using import "common.capnp".InvoiceId;
using import "common.capnp".CustomUInt128;
using import "common.capnp".CustomInt128;
using import "common.capnp".PublicKey;
using import "common.capnp".Hash;
using import "common.capnp".Signature;
using import "common.capnp".RandNonce;

using import "common.capnp".Receipt;
using import "common.capnp".RelayAddress;

using import "report.capnp".Report;
using import "report.capnp".ReportMutation;

using import "index.capnp".RequestFriendsRoute;
using import "index.capnp".ResponseFriendsRoute;


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



#####################################################################

struct AppServerToApp {
    union {
        # Funds
        responseSendFunds @0: ResponseSendFunds;

        # Reports about current state:
        report @1: Report;
        reportMutations @2: List(ReportMutation);

        # Routes:
        responseFriendsRoute @3: ResponseFriendsRoute;

    }
}


struct AppToAppServer {
    union {
        # Set relay address to be used locally (Could be empty)
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

        # Routes:
        requestFriendsRoute @12: RequestFriendsRoute;
    }
}

