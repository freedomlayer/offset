@0x9fb474aba32a63d1;

# A keepalive message wrapper.
# Allows to turn a channel into a channel with keepalive support.
struct KaMessage {
    union {
        keepAlive @0: Void;
        message @1: Data;
    }
} 
