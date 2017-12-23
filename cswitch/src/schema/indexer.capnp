@0x964e69235e372fbb;

using import "common.capnp".CustomUInt128;
using import "common.capnp".CustomUInt256;
using import "common.capnp".CustomUInt512;

struct NeighborsRoute {
        publicKeys @0: List(CustomUInt256);
}

# Updating states chain
#######################

struct ChainLink {
        previousStateHash @0: CustomUInt256;
        newOwnersPublicKeys @1: List(CustomUInt256);
        newIndexersPublicKeys @2: List(CustomUInt256);
        signaturesByOldOwners @3: List(CustomUInt512);
}


# Indexer -> Node
struct RequestUpdateState {
        indexingProviderId @0: CustomUInt128;
        indexingProviderStatesChain @1: List(ChainLink);
}

# Node -> Indexer
struct ResponseUpdateState {
        stateHash @0: CustomUInt256;
}

# Indexer -> Node
struct RoutesToIndexer {
        routes @0: List(NeighborsRoute);
}


# Information collection by indexers
####################################

# (Indexer -> Node) [Empty message]
# struct RequestIndexerInfo {
# }

struct ConnectedFriend {
        sendCapacity @0: CustomUInt128;
        recvCapacity @1: CustomUInt128;
        publicKey @2: CustomUInt256;
}

# Node -> Indexer
struct ResponseIndexerInfo {
        connectedNeighborsList @0: List(CustomUInt256);
        neighborsCommPublicKey @1: CustomUInt256;
        neighborsRecentTimestamp @2: CustomUInt128;
        connectedFriendsList @3: List(ConnectedFriend);
        friendsCommPublicKey @4: CustomUInt256;
        friendsRecentTimestamp @5: CustomUInt128;
}

# Requesting information from indexers
######################################

# Node -> Indexer
struct RequestNeighborsRoute {
        sourceNodePublicKey @0: CustomUInt256;
        destinationNodePublicKey @1: CustomUInt256;
}


# Indexer -> Node
struct ResponseNeighborsRoute { 
        routes @0: List(NeighborsRoute);
        destinationCommPublicKey @1: CustomUInt256;
        destinationRecentTimestamp @2: CustomUInt128;
}

# Request a direct route of friends from the source node to the destination
# node.
struct DirectRoute {
        sourceNodePublicKey @0: CustomUInt256;
        destinationNodePublicKey @1: CustomUInt256;
}

# A loop from myself through given friend, back to myself.
# This is used for money rebalance when we owe the friend money.
struct LoopFromFriendRoute {
        friendPublicKey @0: CustomUInt256;
}

# A loop from myself back to myself through given friend.
# This is used for money rebalance when the friend owe us money.
struct LoopToFriendRoute {
        friendPublicKey @0: CustomUInt256;
}

# Node -> Indexer
struct RequestFriendsRoute { 
        routeType :union {
                direct @0: DirectRoute;
                loopFromFriend @1: LoopFromFriendRoute;
                loopToFriend @2: LoopToFriendRoute;
        }
}

struct FriendsRoute {
        publicKeys @0: List(CustomUInt256);
        capacity @1: UInt64;
        # Maximum amount of credit we can push through this route
}


# Indexer -> Node
struct ResponseFriendsRoute {
        routes @0: List(FriendsRoute);
        destinationCommPublicKey @1: CustomUInt256;
        destinationRecentTimestamp @2: CustomUInt128;
}
