@0xc9191c4889ae128d;

using import "common.capnp".CustomUInt128;
using import "common.capnp".CustomUInt256;
using import "common.capnp".CustomUInt512;
using import "networker.capnp".NeighborsRoute;
using import "funder.capnp".FriendsRoute;
using import "common.capnp".Receipt;

# Initial diffie hellman between an Application and AppManager:

# Application -> AppManager
struct AppRandNonce {
        appRandNonce @0: CustomUInt128;
        appPublicKey @1: CustomUInt256;
}

# AppManager -> Application
struct AppManagerRandNonce {
        appManagerRandNonce @0: CustomUInt128;
        appManagerPublicKey @1: CustomUInt256;
}


# Application -> AppManager
struct AppDh {
        appDhPublicKey @0: CustomUInt256;
        appManagerRandNonce @1: CustomUInt128;
        # This is the nonce previously sent by AppManager.
        appKeySalt @2: CustomUInt256;
        signature @3: CustomUInt512;
}

# AppManager -> Application
struct AppManagerDh {
        appManagerDhPublicKey @0: CustomUInt256;
        appRandNonce @1: CustomUInt128;
        # This is the nonce previously sent by the Application.
        appManagerKeySalt @2: CustomUInt256;
        signature @3: CustomUInt512;
}

# Summary of initial Diffie Hellman messages:

# Messages that App can send during Diffie Hellman:
struct AppDhMessage {
    union {
        appRandNonce @0: AppRandNonce;
        appDh        @1: AppDh;
    }
}

# Messages that the AppManager can send during Diffie Hellman:
struct AppManagerDhMessage {
    union {
        appManagerRandNonce @0: AppManagerRandNonce;
        appManagerDh        @1: AppManagerDh;
    }
}


# Interface with IndexerClient
###############################


# IndexerClient -> AppManager
struct RequestSendMessage {
        requestId @0: CustomUInt128;
        maxResponseLength @1: UInt32;
        processingFeeProposal @2: UInt64;
        route @3: NeighborsRoute;
        requestContent @4: Data;
}

# AppManager -> IndexerClient
struct ResponseSendMessage {
        requestId @0: CustomUInt128;
        processingFeeCollected @1: UInt64;
        responseContent @2: Data;
}

# MessageReceived and RespondIncomingMessage are the same as above.

# AppManager -> IndexerClient
struct RequestNeighborsRoute {
        sourceNodePublicKey @0: CustomUInt256;
        destNodePublicKey @1: CustomUInt256;
}


# IndexerClient -> AppManager
struct ResponseNeighborsRoute {
        routes @0: List(NeighborsRoute);
}


# Request a direct route of friends from the source node to the destination
# node.
struct DirectRoute {
        sourceNodePublicKey @0: CustomUInt256;
        destNodePublicKey @1: CustomUInt256;
}

# A loop from myself through given friend, back to myself.
# This is used for money rebalance when we owe the friend money.
# self -> friend -> ... -> ... -> self
struct LoopFromFriendRoute {
        friendPublicKey @0: CustomUInt256;
}

# A loop from myself back to myself through given friend.
# This is used for money rebalance when the friend owe us money.
# self -> ... -> ... -> friend -> self
struct LoopToFriendRoute {
        friendPublicKey @0: CustomUInt256;
}

# AppManager -> IndexerClient
struct RequestFriendsRoute {
        routeType :union {
                direct @0: DirectRoute;
                loopFromFriend @1: LoopFromFriendRoute;
                loopToFriend @2: LoopToFriendRoute;
        }
}

struct FriendsRouteWithCapacity {
        route @0: FriendsRoute;
        capacity @1: CustomUInt128;
}


# IndexerClient -> AppManager
struct ResponseFriendsRoute {
        routes @0: List(FriendsRouteWithCapacity);
}


struct AppManagerToIndexerClient {
    union {
        responseSendMessage @0: ResponseSendMessage;
        messageReceived @1: RequestSendMessage;
        requestNeighborsRoute @2: RequestNeighborsRoute;
        requestFriendsRoute @3: RequestFriendsRoute;
    }
}

struct IndexerClientToAppManager {
    union {
        requestSendMessage @0: RequestSendMessage;
        respondIncomingMessage @1: ResponseSendMessage;
        responseNeighborsRoute @2: ResponseNeighborsRoute;
        responseFriendsRoute @3: ResponseFriendsRoute;
    }
}


# Interface with an Application
###############################


struct RequestSendFund {
        requestId @0: CustomUInt128;
        destPayment @1: CustomUInt128;
        route @2: FriendsRoute;
        invoiceId @3: CustomUInt256;
}

struct ResponseSendFund {
        requestId @0: CustomUInt128;
        receipt @1: Receipt;
}

struct ReceiptAck {
        requestId @0: CustomUInt128;
        receiptHash @1: CustomUInt256;
}


# Application -> AppManager
struct OpenNeighbor {
        # TODO
}

# Application -> AppManager
struct CloseNeighbor {
        # TODO
}

# Application -> AppManager
struct AddNeighbor {
        # TODO
}

# Application -> AppManager
struct RemoveNeighbor {
        # TODO
}

# Application -> AppManager
struct EnableNeighbor {
        # TODO
}

# Application -> AppManager
struct DisableNeighbor {
        # TODO
}

# Application -> AppManager
struct SetNeighborRemoteMaxDebt {
        # TODO
}

# Application -> AppManager
struct SetNeighborMaxTokenChannels {
        # TODO
}


# Application -> AppManager
struct ResetNeighborChannel {
        # TODO
}

# AppManager -> Application
struct NeighborStateUpdate {
        # TODO
}





# Application -> AppManager
struct OpenFriend {
        # TODO
}

# Application -> AppManager
struct CloseFriend {
        # TODO
}

# Application -> AppManager
struct AddFriend {
        # TODO
}

# Application -> AppManager
struct RemoveFriend {
        # TODO
}

# Application -> AppManager
struct EnableFriend {
        # TODO
}

# Application -> AppManager
struct DisableFriend {
        # TODO
}

# Application -> AppManager
struct SetFriendRemoteMaxDebt {
        # TODO
}

# Application -> AppManager
struct ResetFriendChannel {
        # TODO
}

# AppManager -> Application
struct FriendStateUpdate {
        # TODO
}

struct AppManagerToApp {
    union {
        # Messages
        responseSendMessage @0: ResponseSendMessage;
        messageReceived @1: RequestSendMessage;

        # Funds
        responseSendFund @2: ResponseSendFund;

        # Neighbors management:
        neighborStateUpdate @3: NeighborStateUpdate;

        # Friends management:
        friendStateUpdate @4: FriendStateUpdate;

        # Routes management:
        responseNeighborsRoute @5: ResponseNeighborsRoute;
        responseFriendsRoute @6: ResponseFriendsRoute;

    }
}

struct AppToAppManager {
    union {
        # Messages
        requestSendMessage @0: RequestSendMessage;
        respondIncomingMessage @1: ResponseSendMessage;

        # Funds
        requestSendFunds @2: RequestSendFund;
        receiptAck @3: ReceiptAck;

        # Neighbors management
        openNeighbor @4: OpenNeighbor;
        closeNeighbor @5: CloseNeighbor;
        addNeighbor @6: AddNeighbor;
        removeNeighbor @7: RemoveNeighbor;
        enableNeighbor @8: EnableNeighbor;
        disableNeighbor @9: DisableNeighbor;
        setNeighborRemoteMaxDebt @10: SetNeighborRemoteMaxDebt;
        setNeighborMaxTokenChannels @11: SetNeighborMaxTokenChannels;
        resetNeighborChannel @12: ResetNeighborChannel;

        # Friends management
        openFriend @13: OpenFriend;
        closeFriend @14: CloseFriend;
        addFriend @15: AddFriend;
        removeFriend @16: RemoveFriend;
        enableFriend @17: EnableFriend;
        disableFriend @18: DisableFriend;
        setFriendRemoteMaxDebt @19: SetFriendRemoteMaxDebt;
        resetFriendChannel @20: ResetFriendChannel;

        # Routes management:
        requestNeighborsRoute @21: RequestNeighborsRoute;
        requestFriendsRoute @22: RequestFriendsRoute;
    }
}



# Interface with Networker
##########################

struct AppManagerToNetworker {
    union {
        # Messages
        requestSendMessage @0: RequestSendMessage;
        respondIncomingMessage @1: ResponseSendMessage;

        # Neighbors management
        openNeighbor @2: OpenNeighbor;
        closeNeighbor @3: CloseNeighbor;
        addNeighbor @4: AddNeighbor;
        removeNeighbor @5: RemoveNeighbor;
        enableNeighbor @6: EnableNeighbor;
        disableNeighbor @7: DisableNeighbor;
        setNeighborRemoteMaxDebt @8: SetNeighborRemoteMaxDebt;
        setNeighborMaxTokenChannels @9: SetNeighborMaxTokenChannels;
        resetNeighborChannel @10: ResetNeighborChannel;

        # Routes management:
        responseNeighborsRoute @11: ResponseNeighborsRoute;
    }
}


struct NetworkerToAppManager {
    union {
        # Messages
        responseSendMessage @0: ResponseSendMessage;
        messageReceived @1: RequestSendMessage;

        # Neighbors management:
        neighborStateUpdate @2: NeighborStateUpdate;

        # Routes management:
        requestNeighborsRoute @3: RequestNeighborsRoute;
    }
}

# Interface with Funder
#######################


struct AppManagerToFunder {
    union {
        # Funds
        requestSendFunds @0: RequestSendFund;
        receiptAck @1: ReceiptAck;

        # Friends management
        openFriend @2: OpenFriend;
        closeFriend @3: CloseFriend;
        addFriend @4: AddFriend;
        removeFriend @5: RemoveFriend;
        enableFriend @6: EnableFriend;
        disableFriend @7: DisableFriend;
        setFriendRemoteMaxDebt @8: SetFriendRemoteMaxDebt;
        resetFriendChannel @9: ResetFriendChannel;

        # Routes management:
        requestFriendsRoute @10: RequestFriendsRoute;

    }
}


struct FunderToAppManager {
    union {
        # Funds
        responseSendFund @0: ResponseSendFund;

        # Friends management:
        friendStateUpdate @1: FriendStateUpdate;

        # Routes management:
        responseFriendsRoute @2: ResponseFriendsRoute;
    }
}
