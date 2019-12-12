@0xccef24a2bc5520ea;

using import "common.capnp".PublicKey;


# First message sent after a connection was encrypted.
# This message will determine the context of the connection.
# This message can be sent only once per encrypted connection.
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

# Client -> Relay
struct RejectConnection {
        publicKey @0: PublicKey;
        # Reject incoming connection by PublicKey
}

# Relay -> Client
struct IncomingConnection {
        publicKey @0: PublicKey;
        # Incoming Connection public key
}
