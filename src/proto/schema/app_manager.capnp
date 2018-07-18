@0xc9191c4889ae128d;

using import "common.capnp".CustomUInt128;
using import "common.capnp".CustomUInt256;
using import "common.capnp".CustomUInt512;
using import "networker.capnp".NeighborsRoute;
using import "funder.capnp".FriendsRoute;

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
        requestSendMessage @0: RequestSendMessage;
        respondIncomingMessage @1: ResponseSendMessage;
        requestNeighborsRoute @2: RequestNeighborsRoute;
        requestFriendsRoute @3: RequestFriendsRoute;
    }
}

struct IndexerClientToAppManager {
    union {
        responseSendMessage @0: ResponseSendMessage;
        messageReceived @1: RequestSendMessage;
        responseNeighborsRoute @2: ResponseNeighborsRoute;
        responseFriendsRoute @3: ResponseFriendsRoute;
    }
}


# Interface with an App
########################


