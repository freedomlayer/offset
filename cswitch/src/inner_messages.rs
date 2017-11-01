
// TODO: Find a good crypto library to use for short signatures.

const UID_LEN: usize = 16;
const PUBLIC_KEY_LEN: usize = 32;

struct Uid([u8; UID_LEN]);

// Helper structs
// --------------

struct NodePublicKey([u8; PUBLIC_KEY_LEN]);

struct ChannelerAddress {
    // TODO: Ipv4 or Ipv6 address
}

struct NeighborInfo {
    neighbor_public_key: NodePublicKey,
    neighbor_address: ChannelerAddress,
    max_channels: u32,  // Maximum amount of token channels
    token_channel_capacity: u64,    // Capacity per token channel
}

struct NodesRoute {
    route: Vec<u32>,
}



// Channeler to Networker
// ----------------------

struct ChannelOpened {
    channel_uid: Uid,
    remote_public_key: NodePublicKey, // Public key of remote side
    locally_initialized: bool, // Was this channel initiated by this end.
}

struct ChannelClosed {
    channel_uid: Uid,
}


struct ChannelMessageReceived {
    channel_uid: Uid,
    message_content: Vec<u8>,
}

struct ResponseNeighborsRelationList {
    neighbors_relation_list: Vec<NeighborInfo>,
}

enum ChannelerToNetworker {
    ChannelOpened(ChannelOpened),
    ChannelClosed(ChannelClosed),
    ChannelMessageReceived(ChannelMessageReceived),
    ResponseNeighborsRelationList(ResponseNeighborsRelationList),
}


// Networker to Channeler
// ----------------------

struct SendChannelMessage {
    channel_uid: Uid,
    message_content: Vec<u8>,
}

struct CloseChannel {
    channel_uid: Uid,
}

struct AddNeighborRelation {
    neighbor_info: NeighborInfo,
}


struct RemoveNeighborRelation {
    neighbor_public_key: NodePublicKey,
}

struct SetMaxChannels {
    neighbor_public_key: NodePublicKey,
    max_channels: u32,
}

struct RequestNeighborsRelationList {
}

enum NetworkerToChanneler {
    SendChannelMessage(SendChannelMessage),
    CloseChannel(CloseChannel),
    AddNeighborRelation(AddNeighborRelation),
    RemoveNeighborRelation(RemoveNeighborRelation),
    SetMaxChannels(SetMaxChannels),
    RequestNeighborsRelationList(RequestNeighborsRelationList),
}

// Indexer client to Networker
// ---------------------------


struct RequestSendMessage {
    request_id: Uid,
    message_content: Vec<u8>,
    nodes_route: NodesRoute,
    max_response_length: u64,
    processing_fee: u64,
    delivery_fee: u64,
}

struct IndexerAnnounce {
    // TODO
}

// Networker to Indexer client
// ---------------------------

enum ResponseSendMessageContent {
    Success(Vec<u8>),
    Failure,
}

struct ResponseSendMessage {
    request_id: Uid,
    content: ResponseSendMessageContent,
}

enum NotifyStructureChange {
    NeighborAdded(NodePublicKey),
    NeighborRemoved(NodePublicKey),
}

struct MessageReceived {
    message_content: Vec<u8>,
}


