@0x964e69235e372fbb;

using import "common.capnp".CustomUInt128;
using import "common.capnp".CustomUInt256;
using import "common.capnp".CustomUInt512;

struct NeighborsRoute {
        publicKeys @0: List(CustomUInt256);
}

struct IndexerRoute {
        neighborsRoute @0: NeighborsRoute;
        appPort @1: UInt32;
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
# This message may be accepted from any node.

# Node -> Indexer
struct ResponseUpdateState {
        stateHash @0: CustomUInt256;
}

# Indexer -> Node
struct RoutesToIndexers {
        indexingProviderId @0: CustomUInt128;
        routes @1: List(IndexerRoute);
        requestPrice @2: UInt64;
}
# This message should be accepted only if the sender is an indexer working for
# indexingProviderId. Otherwise it will be discarded.


# Information collection by indexers
####################################

# Done by direct requests at funder.capnp and networker.capnp


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
        # destinationCommPublicKey @1: CustomUInt256;
        # destinationRecentTimestamp @2: CustomUInt128;
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
}
