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
using import "common.capnp".IndexServerAddress;

using import "report.capnp".NodeReport;
using import "report.capnp".NodeReportMutation;

using import "index.capnp".RequestRoutes;
using import "index.capnp".RouteWithCapacity;


# Interface between AppServer and an Application
################################################

struct UserRequestSendFunds {
        requestId @0: Uid;
        route @1: FriendsRoute;
        invoiceId @2: InvoiceId;
        destPayment @3: CustomUInt128;
}

struct ResponseReceived {
        requestId @0: Uid;
        result: union {
                success @1: Receipt;
                failure @2: PublicKey; # Reporting public key
        }
}

struct ReceiptAck {
        requestId @0: Uid;
        receiptSignature @1: Signature;
}

# Application -> AppServer
struct AddFriend {
        friendPublicKey @0: PublicKey;
        relays @1: List(RelayAddress);
        name @2: Text;
        balance @3: CustomInt128;
}

# Application -> AppServer
struct SetFriendName {
        friendPublicKey @0: PublicKey;
        name @1: Text;
}

struct SetFriendRelays {
        friendPublicKey @0: PublicKey;
        relays @1: List(RelayAddress);
}

# Application -> AppServer
struct SetFriendRemoteMaxDebt {
        friendPublicKey @0: PublicKey;
        remoteMaxDebt @1: CustomUInt128;
}

# Application -> AppServer
struct ResetFriendChannel {
        friendPublicKey @0: PublicKey;
        resetToken @1: Signature;
}

struct ResponseRoutesResult {
        union {
                success @0: List(RouteWithCapacity);
                failure @1: Void;
        }
}

struct ClientResponseRoutes {
        requestId @0: Uid;
        result @1: ResponseRoutesResult;
}


#####################################################################

struct AppPermissions {
        reports @0: Bool;
        # Receives reports about state
        routes @1: Bool;
        # Can request routes
        sendFunds @2: Bool;
        # Can send credits
        config @3: Bool;
        # Can configure friends
}

struct AppServerToApp {
    union {
        # Funds
        responseReceived @0: ResponseReceived;

        # Reports about current state:
        report @1: NodeReport;
        reportMutations @2: List(NodeReportMutation);

        # Routes:
        responseRoutes @3: ClientResponseRoutes;

    }
}


struct AppToAppServer {
    union {
        # Set relay address to be used locally (Could be empty)
        setRelays @0: List(RelayAddress);

        # Sending Funds:
        requestSendFunds @1: UserRequestSendFunds;
        receiptAck @2: ReceiptAck;

        # Friends management
        addFriend @3: AddFriend;
        setFriendRelays @4: SetFriendRelays;
        setFriendName @5: SetFriendName;
        removeFriend @6: PublicKey;
        enableFriend @7: PublicKey;
        disableFriend @8: PublicKey;
        openFriend @9: PublicKey;
        closeFriend @10: PublicKey;
        setFriendRemoteMaxDebt @11: SetFriendRemoteMaxDebt;
        resetFriendChannel @12: ResetFriendChannel;

        # Routes:
        requestRoutes @13: RequestRoutes;

        # Index servers management:
        addIndexServer @14: IndexServerAddress;
        removeIndexServer @15: IndexServerAddress;
    }
}

