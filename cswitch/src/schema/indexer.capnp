@0x964e69235e372fbb;

# TODO: Can we have a single file that has the custom types for 128,256 and
# 512? Possibly combine it for channeler.capnp and indexer.capnp

# A custom made 128 bit data structure.
struct CustomUInt128 {
        x0 @0: UInt64;
        x1 @1: UInt64;
}

# A custom made 256 bit data structure.
struct CustomUInt256 {
        x0 @0: UInt64;
        x1 @1: UInt64;
        x2 @2: UInt64;
        x3 @3: UInt64;
}

# A custom made 512 bit data structure.
struct CustomUInt512 {
        x0 @0: UInt64;
        x1 @1: UInt64;
        x2 @2: UInt64;
        x3 @3: UInt64;
        x4 @4: UInt64;
        x5 @5: UInt64;
        x6 @6: UInt64;
        x7 @7: UInt64;
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
        indexingProviderName @0: CustomUInt128;
        indexingProviderStatesChain @1: List(ChainLink);
}

# Node -> Indexer
struct ResponseUpdateState {
        stateHash @0: CustomUInt256;
}

# Indexer -> Node
struct RouteToIndexer {
        nodesPublicKeys @0: List(CustomUInt256);
}


# Information collection by indexers
####################################

# (Indexer -> Node) [Empty message]
# struct RequestIndexerInfo {
# }

struct ConnectedFriend {
        pushCredits @0: CustomUInt128;
        publicKey @1: CustomUInt256;
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

struct Route {
        publicKeys @0: List(CustomUInt256);
}

# Indexer -> Node
struct ResponseNeighborsRoute { 
        routes @0: List(Route);
        destinationCommPublicKey @1: CustomUInt256;
        destinationRecentTimestamp @2: CustomUInt128;
}

# Node -> Indexer
struct RequestFriendsRoute { 
        sourceNodePublicKey @0: CustomUInt256;
        secondNodePublicKey @1: CustomUInt256;
        beforeLastNodePublicKey @2: CustomUInt256;
        destinationNodePublicKey @3: CustomUInt256;
}

# Indexer -> Node
struct ResponseFriendsRoute {
        routes @0: List(Route);
        destinationCommPublicKey @1: CustomUInt256;
        destinationRecentTimestamp @2: CustomUInt128;
}
