@0xc9191c4889ae128d;

using import "common.capnp".CustomUInt128;
using import "common.capnp".CustomUInt256;
using import "common.capnp".CustomUInt512;
using import "networker.capnp".NeighborsRoute;
using import "networker.capnp".NetworkerSendPrice;
using import "networker_crypter.capnp".DestinationPort;
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

struct RequestPath {
        route @0: NeighborsRoute;
}

struct ResponsePath {
        pathId @0: CustomUInt128;
}

struct RequestSendMessage {
        requestId @0: CustomUInt128;
        pathId @1: CustomUInt128;
        destPort @2: DestinationPort;
        maxResponseLength @3: UInt32;
        processingFeeProposal @4: UInt64;
        requestContent @5: Data;
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
        responsePath @0: ResponsePath;
        responseSendMessage @1: ResponseSendMessage;
        messageReceived @2: RequestSendMessage;
        requestNeighborsRoute @3: RequestNeighborsRoute;
        requestFriendsRoute @4: RequestFriendsRoute;
    }
}

struct IndexerClientToAppManager {
    union {
        requestPath @0: RequestPath;
        requestSendMessage @1: RequestSendMessage;
        respondIncomingMessage @2: RespondSendMessage;
        responseNeighborsRoute @3: ResponseNeighborsRoute;
        responseFriendsRoute @4: ResponseFriendsRoute;
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
        requestPath @0: RequestPath;
        requestSendMessage @1: RequestSendMessage;
        respondIncomingMessage @2: RespondSendMessage;

        # Funds
        requestSendFunds @3: RequestSendFunds;
        receiptAck @4: ReceiptAck;

        # Neighbors management
        addNeighbor @5: AddNeighbor;
        removeNeighbor @6: RemoveNeighbor;
        openNeighborChannel @7: OpenNeighborChannel;
        closeNeighborChannel @8: CloseNeighborChannel;
        enableNeighbor @9: EnableNeighbor;
        disableNeighbor @10: DisableNeighbor;
        setNeighborRemoteMaxDebt @11: SetNeighborRemoteMaxDebt;
        setNeighborMaxTokenChannels @12: SetNeighborMaxTokenChannels;
        setNeighborAddr @13: SetNeighborAddr;
        resetNeighborChannel @14: ResetNeighborChannel;

        # Friends management
        addFriend @15: AddFriend;
        removeFriend @16: RemoveFriend;
        openFriend @17: OpenFriend;
        closeFriend @18: CloseFriend;
        enableFriend @19: EnableFriend;
        disableFriend @20: DisableFriend;
        setFriendRemoteMaxDebt @21: SetFriendRemoteMaxDebt;
        resetFriendChannel @22: ResetFriendChannel;

        # Routes management:
        requestNeighborsRoute @23: RequestNeighborsRoute;
        requestFriendsRoute @24: RequestFriendsRoute;
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
        requestPath @0: RequestPath;
        requestSendMessage @1: RequestSendMessage;
        respondIncomingMessage @2: RespondSendMessage;

        # Neighbors management
        addNeighbor @3: AddNeighbor;
        removeNeighbor @4: RemoveNeighbor;
        openNeighborChannel @5: OpenNeighborChannel ;
        closeNeighborChannel @6: CloseNeighborChannel;
        enableNeighbor @7: EnableNeighbor;
        disableNeighbor @8: DisableNeighbor;
        setNeighborRemoteMaxDebt @9: SetNeighborRemoteMaxDebt;
        setNeighborMaxTokenChannels @10: SetNeighborMaxTokenChannels;
        setNeighborAddr @11: SetNeighborAddr;
        resetNeighborChannel @12: ResetNeighborChannel;

        # Routes management:
        responseFriendsRoute @13: ResponseFriendsRoute;

    }
}


struct NetworkerToAppManager {
    union {
        # Messages
        responsePath @0: ResponsePath;
        responseSendMessage @1: ResponseSendMessage;
        messageReceived @2: RequestSendMessage;

        # Neighbors management:
        neighborStateUpdate @3: NeighborStateUpdate;

        # Routes management:
        requestFriendsRoute @4: RequestFriendsRoute;
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
