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

# TODO: Add a destination port for request send message.
# Should also be added in Rust's structs interface.

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

struct RespondSendMessage {
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
        respondIncomingMessage @1: RespondSendMessage;
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
        neighborAddr: union {
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
struct SetNeighborAddr {
        neighborPublicKey @0: CustomUInt256;
        neighborAddr: union {
                socketAddr @1: SocketAddr;
                none @2: Void;
        }
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
        neighborAddr: union {
                socketAddr @0: SocketAddr;
                none @1: Void;
        }
        maxChannels @2: UInt16;
        status @3: NeighborStatus;
        # When reading this field, make sure that there are no duplicates of channelIndex!
        # Idealy this would have been a HashMap (with channelIndex as key), and not a List.
}

struct NeighborTokenChannelUpdated {
        channelIndex @0: UInt16;
        balance @1: Int64;
        localMaxDebt @2: UInt64;
        remoteMaxDebt @3: UInt64;
        localPendingDebt @4: UInt64;
        remotePendingDebt @5: UInt64;
        requestsStatus: union {
                sendPrice @6: NetworkerSendPrice;
                closed @7: Void;
        }
}

struct NeighborTokenChannelInconsistent {
        channelIndex @0: UInt16;
        currentToken @1: CustomUInt256;
        balanceForReset @2: Int64;
}

# AppManager -> Application
struct NeighborStateUpdate {
        neighborPublicKey @0: CustomUInt256;
        union {
                neighborUpdated @1: NeighborUpdated;
                neighborRemoved @2: Void;
                neighborTokenChannelUpdated @3: NeighborTokenChannelUpdated;
                neighborTokenChannelInconsistent @4: NeighborTokenChannelInconsistent;
        }
} 

# Application -> AppManager
struct AddFriend {
        friendPublicKey @0: CustomUInt256;
        remoteMaxDebt @1: CustomUInt128;
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
        friendPublicKey @0: CustomUInt256;
        currentToken @1: CustomUInt256;
        balanceForReset @2: CustomUInt128;
}

struct FriendUpdated {
        balance @0: CustomUInt128;
        localMaxDebt @1: CustomUInt128;
        remoteMaxDebt @2: CustomUInt128;
        localPendingDebt @3: CustomUInt128;
        remotePendingDebt @4: CustomUInt128;
        requestsStatus: union {
                sendPrice @5: FunderSendPrice;
                closed @6: Void;
        }
}

struct FriendInconsistent {
        currentToken @0: CustomUInt256;
        balanceForReset @1: CustomUInt128;
}

# AppManager -> Application
struct FriendStateUpdate {
        friendPublicKey @0: CustomUInt256;
        union {
                friendUpdated @1: FriendUpdated;
                friendRemoved @2: Void;
                friendInconsistent @3: FriendInconsistent;
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
        respondIncomingMessage @1: RespondSendMessage;

        # Funds
        requestSendFunds @2: RequestSendFunds;
        receiptAck @3: ReceiptAck;

        # Neighbors management
        addNeighbor @4: AddNeighbor;
        removeNeighbor @5: RemoveNeighbor;
        openNeighborChannel @6: OpenNeighborChannel;
        closeNeighborChannel @7: CloseNeighborChannel;
        enableNeighbor @8: EnableNeighbor;
        disableNeighbor @9: DisableNeighbor;
        setNeighborRemoteMaxDebt @10: SetNeighborRemoteMaxDebt;
        setNeighborMaxTokenChannels @11: SetNeighborMaxTokenChannels;
        setNeighborAddr @12: SetNeighborAddr;
        resetNeighborChannel @13: ResetNeighborChannel;

        # Friends management
        addFriend @14: AddFriend;
        removeFriend @15: RemoveFriend;
        openFriend @16: OpenFriend;
        closeFriend @17: CloseFriend;
        enableFriend @18: EnableFriend;
        disableFriend @19: DisableFriend;
        setFriendRemoteMaxDebt @20: SetFriendRemoteMaxDebt;
        resetFriendChannel @21: ResetFriendChannel;

        # Routes management:
        requestNeighborsRoute @22: RequestNeighborsRoute;
        requestFriendsRoute @23: RequestFriendsRoute;
    }
}



# Following interfaces should be written as Rust structures. 
# They are only an example, and should not be used!
# They are internal to CSwitch:

# Interface with Networker
##########################

struct AppManagerToNetworker {
    union {
        # Messages
        requestSendMessage @0: RequestSendMessage;
        respondIncomingMessage @1: RespondSendMessage;

        # Neighbors management
        addNeighbor @2: AddNeighbor;
        removeNeighbor @3: RemoveNeighbor;
        openNeighborChannel @4: OpenNeighborChannel ;
        closeNeighborChannel @5: CloseNeighborChannel;
        enableNeighbor @6: EnableNeighbor;
        disableNeighbor @7: DisableNeighbor;
        setNeighborRemoteMaxDebt @8: SetNeighborRemoteMaxDebt;
        setNeighborMaxTokenChannels @9: SetNeighborMaxTokenChannels;
        setNeighborAddr @10: SetNeighborAddr;
        resetNeighborChannel @11: ResetNeighborChannel;

        # Routes management:
        responseFriendsRoute @12: ResponseFriendsRoute;

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
        requestFriendsRoute @3: RequestFriendsRoute;
    }
}

# Interface with Funder
#######################


struct AppManagerToFunder {
    union {
        # Funds
        requestSendFunds @0: RequestSendFunds;
        receiptAck @1: ReceiptAck;

        # Friends management
        addFriend @2: AddFriend;
        removeFriend @3: RemoveFriend;
        openFriend @4: OpenFriend;
        closeFriend @5: CloseFriend;
        enableFriend @6: EnableFriend;
        disableFriend @7: DisableFriend;
        setFriendRemoteMaxDebt @8: SetFriendRemoteMaxDebt;
        resetFriendChannel @9: ResetFriendChannel;

        # Routes management:
        responseNeighborsRoute @10: ResponseNeighborsRoute;
    }
}


struct FunderToAppManager {
    union {
        # Funds
        responseSendFunds @0: ResponseSendFunds;

        # Friends management:
        friendStateUpdate @1: FriendStateUpdate;

        # Routes management:
        requestNeighborsRoute @2: RequestNeighborsRoute;

    }
}
