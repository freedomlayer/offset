@0xccef24a2bc5520ea;

using import "common.capnp".PublicKey;


# First message sent after a connection was encrypted.
# This message will determine the context of the connection.
# This messeag can be sent only once per encrypted connection.
struct InitConnection {
    union {
        listen @0: Void;
        # Listen to connections
        accept @1: PublicKey;
        # Accepting connection from <PublicKey>
        connect @2: PublicKey;
        # Request for a connection to <PublicKey> 
    }
}

struct RelayListenIn {
    union {
        keepAlive @0: Void;
        rejectConnection @1: PublicKey;
        # Reject incoming connection by PublicKey
    }
}

struct RelayListenOut {
    union {
        keepAlive @0: Void;
        incomingConnection @1: PublicKey;
        # Incoming Connection public key
    }
}

struct TunnelMessage {
    union {
        keepAlive @0: Void;
        message @1: Data;
        # Send a message through the Tunnel
    }
} 
