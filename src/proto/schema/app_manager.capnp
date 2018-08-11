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

struct ExchangeRandNonce {
        randNonce @0: CustomUInt128;
        publicKey @1: CustomUInt256;
}

struct Signed {
    data @0: Data;
    signature @1: CustomUInt512;
}

struct ExchangeDh {
        dhPublicKey @0: CustomUInt256;
        randNonce @1: CustomUInt128;
        # This is the nonce previously sent by the remote side.
        keySalt @2: CustomUInt256;
}


# Interface with IndexerClient
###############################

# TODO: Add a destination port for request send message.
# Should also be added in Rust's structs interface.

struct RequestPath {
        pathRequestId @0: CustomUInt128;
        route @1: NeighborsRoute;
        pathFeeProposal @2: UInt64;
        # Maximum amount of credits we are willing to pay for opening this
        # secure path.
}

struct ResponsePath {
        pathRequestId @0: CustomUInt128;
        union {
                pathId @1: CustomUInt128;
                # Opening a secure path is successful.
                proposalTooLow @2: UInt64;
                # Proposal was too low, this is what the remote side asked for.
                failure @3: Void;
                # We could not reach the remote side using the provided route,
                # or something strange happened.
        }
}

struct ClosePath {
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

struct MessageReceived {
        messageId @0: CustomUInt128;
        route @1: NeighborsRoute;
        maxResponseLength @2: UInt32;
        processingFeeProposal @3: UInt64;
        requestContent @4: Data;
}

struct FailureSendMessage {
        union {
                unreachable @0: CustomUInt256;
                # reportingPublicKey
                noListeningApp @1: Void;
                # remote exists, but no application currently listens on the
                # requested port.
                remoteLostKey @2: Void;
                # Remote's Crypter lost the key, so he can not decrypt the message.
                noSuchPath @3: Void;
                # AppManager doesn't know about such path.
        }
}

struct SuccessSendMessage {
        processingFeeCollected @0: UInt64;
        responseContent @1: Data;
}

# When AppManager receives a MessageReceived from the Networker, it should
# first search for the relevant application (Using the provided port).  If the
# application does not exist, it sends back to Networker a
# MessageReceivedResponse with data = ResponseData::noListeningApp.
# 
# If, on the other hand, the appliation exists, AppManager will pass the
# MessageReceived to the Application, and wait for a reply of
# RespondIncomingMessage from the Application. When this arrives, AppManager
# wraps it inside a ResponseData::appData(...) and send it as the data of
# RespondIncomingMessage back to the Networker.
struct ResponseData {
        union {
                noListeningApp @0: Void;
                appData @1: Data;
        }
}

# AppManager -> IndexerClient
struct ResponseSendMessage {
        requestId @0: CustomUInt128;
        pathId @1: CustomUInt128;
        response: union {
                success @2: SuccessSendMessage;
                failure @3: FailureSendMessage;
        }
}

struct RespondIncomingMessage {
        messageId @0: CustomUInt128;
        processingFeeCollected @1: UInt64;
        responseContent @2: Data;
}


# AppManager -> IndexerClient
struct RequestNeighborsRoute {
        requestRouteId @0: CustomUInt128;
        sourceNodePublicKey @1: CustomUInt256;
        destNodePublicKey @2: CustomUInt256;
}


# IndexerClient -> AppManager
struct ResponseNeighborsRoute { 
        responseRouteId @0: CustomUInt128;
        routes @1: List(NeighborsRoute);
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
        requestRouteId @0: CustomUInt128;
        routeType :union {
                direct @1: DirectRoute;
                loopFromFriend @2: LoopFromFriendRoute;
                loopToFriend @3: LoopToFriendRoute;
        }
}

struct FriendsRouteWithCapacity {
        route @0: FriendsRoute;
        capacity @1: CustomUInt128;
}


# IndexerClient -> AppManager
struct ResponseFriendsRoute {
        requestRouteId @0: CustomUInt128;
        routes @1: List(FriendsRouteWithCapacity);
}


struct AppManagerToIndexerClient {
    union {
        responsePath @0: ResponsePath;
        responseSendMessage @1: ResponseSendMessage;
        messageReceived @2: MessageReceived;
        requestNeighborsRoute @3: RequestNeighborsRoute;
        requestFriendsRoute @4: RequestFriendsRoute;
    }
}

struct IndexerClientToAppManager {
    union {
        requestPath @0: RequestPath;
        closePath @1: ClosePath;
        requestSendMessage @2: RequestSendMessage;
        respondIncomingMessage @3: RespondIncomingMessage;
        responseNeighborsRoute @4: ResponseNeighborsRoute;
        responseFriendsRoute @5: ResponseFriendsRoute;
    }
}


# Interface with an Application
###############################


struct RequestSendFunds {
        paymentId @0: CustomUInt128;
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
        paymentId @0: CustomUInt128;
        response: union {
                success @1: SuccessSendFunds;
                failure @2: FailureSendFunds;
        }
}

struct ReceiptAck {
        paymentId @0: CustomUInt128;
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
struct SetIncomingPathFee {
        incomingPathFee @0: UInt64;
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

struct StateUpdate {
        union {
                neighborStateUpdate @0: NeighborStateUpdate;
                friendStateUpdate @1: FriendStateUpdate;
                incomingPathFeeUpdate @2: UInt64;
        }
}

struct AppManagerToApp {
    union {
        # Messages
        responseSendMessage @0: ResponseSendMessage;
        messageReceived @1: MessageReceived;

        # Funds
        responseSendFunds @2: ResponseSendFunds;

        # Neighbors management:
        stateUpdate @3: StateUpdate;

        # Routes management:
        responseNeighborsRoute @4: ResponseNeighborsRoute;
        responseFriendsRoute @5: ResponseFriendsRoute;

    }
}

struct AppToAppManager {
    union {
        # Messages
        requestPath @0: RequestPath;
        closePath @1: ClosePath;
        requestSendMessage @2: RequestSendMessage;
        respondIncomingMessage @3: RespondIncomingMessage;

        # Funds
        requestSendFunds @4: RequestSendFunds;
        receiptAck @5: ReceiptAck;

        # Neighbors management
        addNeighbor @6: AddNeighbor;
        removeNeighbor @7: RemoveNeighbor;
        openNeighborChannel @8: OpenNeighborChannel;
        closeNeighborChannel @9: CloseNeighborChannel;
        enableNeighbor @10: EnableNeighbor;
        disableNeighbor @11: DisableNeighbor;
        setNeighborRemoteMaxDebt @12: SetNeighborRemoteMaxDebt;
        setNeighborMaxTokenChannels @13: SetNeighborMaxTokenChannels;
        setNeighborAddr @14: SetNeighborAddr;
        resetNeighborChannel @15: ResetNeighborChannel;

        # Friends management
        addFriend @16: AddFriend;
        removeFriend @17: RemoveFriend;
        openFriend @18: OpenFriend;
        closeFriend @19: CloseFriend;
        enableFriend @20: EnableFriend;
        disableFriend @21: DisableFriend;
        setFriendRemoteMaxDebt @22: SetFriendRemoteMaxDebt;
        resetFriendChannel @23: ResetFriendChannel;

        # Routes management:
        requestNeighborsRoute @24: RequestNeighborsRoute;
        requestFriendsRoute @25: RequestFriendsRoute;
    }
}


# The data we encrypt. Contains random padding (Of variable length) together
# with actual data. This struct is used for any data we encrypt.
# Incremental nonce should be used for encryption/decryption.
struct Plain {
    randPadding @0: Data;
    content :union {
        user      @1: Data;
        keepAlive @2: Void;
    }
}
