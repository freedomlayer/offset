@0xc9191c4889ae128d;

using import "common.capnp".CustomUInt128;
using import "common.capnp".CustomUInt256;
using import "common.capnp".CustomUInt512;
using import "networker.capnp".NeighborsRoute;
using import "networker.capnp".NetworkerSendPrice;
using import "funder.capnp".FriendsRoute;
using import "funder.capnp".FunderSendPrice;
using import "common.capnp".Receipt;

# Initial diffie hellman between an Application and AppManager:
###############################################################

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

struct FailureSendMessage {
        reportingPublicKey @0: CustomUInt256;
}

struct SuccessSendMessage {
        processingFeeCollected @0: UInt64;
        responseContent @1: Data;
}

# AppManager -> IndexerClient
struct ResponseSendMessage {
        requestId @0: CustomUInt128;
        response: union {
                success @1: SuccessSendMessage;
                failure @2: FailureSendMessage;
        }
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


struct RequestSendFunds {
        requestId @0: CustomUInt128;
        destPayment @1: CustomUInt128;
        route @2: FriendsRoute;
        invoiceId @3: CustomUInt256;
}

struct SuccessSendFunds {
        receipt @0: Receipt;
} 

struct FailureSendFunds {
        reportingPublicKey @0: CustomUInt256;
}

struct ResponseSendFunds {
        requestId @0: CustomUInt128;
        response: union {
                success @1: SuccessSendFunds;
                failure @2: FailureSendFunds;
        }
}

struct ReceiptAck {
        requestId @0: CustomUInt128;
        receiptHash @1: CustomUInt256;
}



# IP address and port
struct SocketAddr {
        address @0: UInt32;
        port @1: UInt16;
}

# Application -> AppManager
struct AddNeighbor {
        neighborPublicKey @0: CustomUInt256;
        neighborSocketAddr: union {
                socketAddr @1: SocketAddr;
                none @2: Void;
        }
        maxChannels @3: UInt16;
}

# Application -> AppManager
struct RemoveNeighbor {
        neighborPublicKey @0: CustomUInt256;
}

# Application -> AppManager
struct OpenNeighborChannel {
        neighborPublicKey @0: CustomUInt256;
        channelIndex @1: UInt16;
        sendPrice @2: NetworkerSendPrice;
}

# Application -> AppManager
struct CloseNeighborChannel {
        neighborPublicKey @0: CustomUInt256;
        channelIndex @1: UInt16;
}

# Application -> AppManager
struct EnableNeighbor {
        neighborPublicKey @0: CustomUInt256;
}

# Application -> AppManager
struct DisableNeighbor {
        neighborPublicKey @0: CustomUInt256;
}

# Application -> AppManager
struct SetNeighborRemoteMaxDebt {
        neighborPublicKey @0: CustomUInt256;
        channelIndex @1: UInt16;
        remoteMaxDebt @2: UInt64;
}

# Application -> AppManager
struct SetNeighborMaxTokenChannels {
        neighborPublicKey @0: CustomUInt256;
        maxChannels @1: UInt16;
}

# Application -> AppManager
struct ResetNeighborChannel {
        neighborPublicKey @0: CustomUInt256;
        channelIndex @1: UInt16;
        currentToken @2: CustomUInt256;
        balanceForReset @3: Int64;
}

enum NeighborStatus {
        enabled @0;
        disabled @1;
}

struct NeighborUpdated {
        neighborPublicKey @0: CustomUInt256;
        neighborSocketAddr: union {
                socketAddr @1: SocketAddr;
                none @2: Void;
        }
        maxChannels @3: UInt16;
        status @4: NeighborStatus;
        # When reading this field, make sure that there are no duplicates of channelIndex!
        # Idealy this would have been a HashMap (with channelIndex as key), and not a List.
}

struct NeighborTokenChannelUpdated {
        neighborPublicKey @0: CustomUInt256;
        channelIndex @1: UInt16;
        balance @2: Int64;
        localMaxDebt @3: UInt64;
        remoteMaxDebt @4: UInt64;
        localPendingDebt @5: UInt64;
        remotePendingDebt @6: UInt64;
        requestsStatus: union {
                sendPrice @7: NetworkerSendPrice;
                closed @8: Void;
        }
}

struct NeighborRemoved {
        neighborPublicKey @0: CustomUInt256;
}

# AppManager -> Application
struct NeighborStateUpdate {
    union {
        neighborUpdated @0: NeighborUpdated;
        neighborRemoved @1: NeighborRemoved;
        neighborTokenChannelUpdated @2: NeighborTokenChannelUpdated;
    }
}


# Application -> AppManager
struct AddFriend {
        friendPublicKey @0: CustomUInt256;
}

# Application -> AppManager
struct RemoveFriend {
        friendPublicKey @0: CustomUInt256;
}

# Application -> AppManager
struct OpenFriend {
        friendPublicKey @0: CustomUInt256;
        sendPrice @1: FunderSendPrice;
}

# Application -> AppManager
struct CloseFriend {
        friendPublicKey @0: CustomUInt256;
}

# Application -> AppManager
struct EnableFriend {
        friendPublicKey @0: CustomUInt256;
}

# Application -> AppManager
struct DisableFriend {
        friendPublicKey @0: CustomUInt256;
}

# Application -> AppManager
struct SetFriendRemoteMaxDebt {
        friendPublicKey @0: CustomUInt256;
        remoteMaxDebt @1: CustomUInt128;
}

# Application -> AppManager
struct ResetFriendChannel {
        neighborPublicKey @0: CustomUInt256;
        currentToken @1: CustomUInt256;
        balanceForReset @2: CustomUInt128;
}

struct FriendUpdated {
        friendPublicKey @0: CustomUInt256;
        balance @1: CustomUInt128;
        localMaxDebt @2: CustomUInt128;
        remoteMaxDebt @3: CustomUInt128;
        localPendingDebt @4: CustomUInt128;
        remotePendingDebt @5: CustomUInt128;
        requestsStatus: union {
                sendPrice @6: FunderSendPrice;
                closed @7: Void;
        }
}

struct FriendRemoved {
        friendPublicKey @0: CustomUInt256;
}

# AppManager -> Application
struct FriendStateUpdate {
    union {
        friendUpdated @0: FriendUpdated;
        friendRemoved @1: FriendRemoved;
    }
}

struct AppManagerToApp {
    union {
        # Messages
        responseSendMessage @0: ResponseSendMessage;
        messageReceived @1: RequestSendMessage;

        # Funds
        responseSendFunds @2: ResponseSendFunds;

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
        requestSendFunds @2: RequestSendFunds;
        receiptAck @3: ReceiptAck;

        # Neighbors management
        addNeighbor @4: AddNeighbor;
        removeNeighbor @5: RemoveNeighbor;
        openNeighbor @6: OpenNeighborChannel;
        closeNeighbor @7: CloseNeighborChannel;
        enableNeighbor @8: EnableNeighbor;
        disableNeighbor @9: DisableNeighbor;
        setNeighborRemoteMaxDebt @10: SetNeighborRemoteMaxDebt;
        setNeighborMaxTokenChannels @11: SetNeighborMaxTokenChannels;
        resetNeighborChannel @12: ResetNeighborChannel;

        # Friends management
        addFriend @13: AddFriend;
        removeFriend @14: RemoveFriend;
        openFriend @15: OpenFriend;
        closeFriend @16: CloseFriend;
        enableFriend @17: EnableFriend;
        disableFriend @18: DisableFriend;
        setFriendRemoteMaxDebt @19: SetFriendRemoteMaxDebt;
        resetFriendChannel @20: ResetFriendChannel;

        # Routes management:
        requestNeighborsRoute @21: RequestNeighborsRoute;
        requestFriendsRoute @22: RequestFriendsRoute;
    }
}



# Following interfaces should be written as Rust structures.
# They are internal to CSwitch:
#
#       # Interface with Networker
#       ##########################

#       struct AppManagerToNetworker {
#           union {
#               # Messages
#               requestSendMessage @0: RequestSendMessage;
#               respondIncomingMessage @1: ResponseSendMessage;

#               # Neighbors management
#               openNeighbor @2: OpenNeighbor;
#               closeNeighbor @3: CloseNeighbor;
#               addNeighbor @4: AddNeighbor;
#               removeNeighbor @5: RemoveNeighbor;
#               enableNeighbor @6: EnableNeighbor;
#               disableNeighbor @7: DisableNeighbor;
#               setNeighborRemoteMaxDebt @8: SetNeighborRemoteMaxDebt;
#               setNeighborMaxTokenChannels @9: SetNeighborMaxTokenChannels;
#               resetNeighborChannel @10: ResetNeighborChannel;

#               # Routes management:
#               responseNeighborsRoute @11: ResponseNeighborsRoute;
#           }
#       }


#       struct NetworkerToAppManager {
#           union {
#               # Messages
#               responseSendMessage @0: ResponseSendMessage;
#               messageReceived @1: RequestSendMessage;

#               # Neighbors management:
#               neighborStateUpdate @2: NeighborStateUpdate;

#               # Routes management:
#               requestNeighborsRoute @3: RequestNeighborsRoute;
#           }
#       }

#       # Interface with Funder
#       #######################


#       struct AppManagerToFunder {
#           union {
#               # Funds
#               requestSendFunds @0: RequestSendFunds;
#               receiptAck @1: ReceiptAck;

#               # Friends management
#               openFriend @2: OpenFriend;
#               closeFriend @3: CloseFriend;
#               addFriend @4: AddFriend;
#               removeFriend @5: RemoveFriend;
#               enableFriend @6: EnableFriend;
#               disableFriend @7: DisableFriend;
#               setFriendRemoteMaxDebt @8: SetFriendRemoteMaxDebt;
#               resetFriendChannel @9: ResetFriendChannel;

#               # Routes management:
#               requestFriendsRoute @10: RequestFriendsRoute;

#           }
#       }


#       struct FunderToAppManager {
#           union {
#               # Funds
#               responseSendFunds @0: ResponseSendFunds;

#               # Friends management:
#               friendStateUpdate @1: FriendStateUpdate;

#               # Routes management:
#               responseFriendsRoute @2: ResponseFriendsRoute;
#           }
#       }
