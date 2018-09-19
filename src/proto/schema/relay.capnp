@0xecf75fe173593f90;

using import "common.capnp".CustomUInt128;
using import "common.capnp".CustomUInt256;
using import "common.capnp".CustomUInt512;


# First message sent after a connection was encrypted.
# This message will determine the context of the connection.
# This messeag can be sent only once per encrypted connection.
struct InitConnection {
    union {
        listen @0: Void;
        # Listen to connections
        accept @1: CustomUInt256;
        # Accepting connection from <PublicKey>
        connect @2: CustomUInt256;
        # Request for a connection to <PublicKey>
    }
}

struct RelayListenIn {
    rejectConnection @0: CustomUInt256;
    # Reject incoming connection by PublicKey
}

struct RelayListenOut {
    incomingConnection @0: CustomUInt256;
    # Incoming Connection public key
}

struct ConnectionMessage {
    message @0: Data;
    # Send a message through the connection.
}
