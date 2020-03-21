# Network structure

Offset network consists of a few entities:

- Relay server (`strelay`)
- Index server (`stindex`)
- Node (`stnode`)
- Applications (For example: `stctrl`)

## Example network topology

```text
                  +-------+
                  | Index |
                  +-+---+-+
                    |   |
              +-----+   +------+
              |                |
         +----+--+         +---+---+          +-------+
         | Index +---------+ Index +----------+ Index |
         +-+-----+         +-------+          +-----+-+
           |                                        |
           |                                        +---+
           |     +-------+         +-------+            |    +-------+
           |     | Relay |         | Relay |            |    | Relay |
           |     +--+----+         +-+---+-+            |    +---+---+
           |        |                |   |              |        |
           |+-------+                |   |              |        |
           || +----------------------+   +-----------+  |+-------+
           || |                                      |  ||
         /-++-+-\                                  /-+--++\
         | Node |                                  | Node |
         \-+--+-/                                  \---+--/
           |  |                                        |
        +--+  +---+                                    |
        |         |                                    |
     +--+--+   +--+--+                              +--+--+
     | App |   | App |                              | App |
     +-----+   +-----+                              +-----+

```

All lines in the diagram above represent TCP connections.

## Node

The core payment logic happens inside the offset node.

To operate, a node needs the following information:

- An identity file: A private key used to authenticate the identity of the node.
- A database file: Used to save the current relationships (balances and open payment requests) with other nodes.
- A list of trusted applications and their permissions.

A Node allows applications to connect. Applications must register ahead of time
to be able to connect to the node. The communication interface between a node
and an application allows an application to:

- Obtain information about the node.
- Configure the node
- Request routes
- Send funds

A node needs to be configured to have at least one relay server and one index
server to be able to operate.

## Relay server

In a perfect world nodes should have been able to directly communicate with
each other. However, currently many of the Internet users are not able to
receive connections directly.

As a workaround, the offset network uses Relay servers.
A Relay server is a server that is used as a meeting point for communication between
two nodes. A node connects to a relay server in one of two modes:

- Wait for incoming connections from remote nodes.
- Request to connect to a remote node.

When a node is configured to have a remote node as a friend, it must know one
or more relays on which the remote node is listening for connections.

The relays model is decentralized. Anyone [^1] can run his own relay. However,
we realize that some users might not be willing (or able) to run their own
relay servers. Instead, it is possible to use the services of a public relay.

[^1]: That has his own public address (IP or domain name)

## Index server

Payments in offset rely on pushing credits along a routes of nodes.
We did not manage to find a fully decentralized solution for finding capacity
routes between nodes. As a workaround, we use Index servers.

A node is configured to know one or more index servers, and can ask information
about routes from the index servers. A node also sends to the index servers
periodic updates about his relationship with his friends.

The index servers form a **Federation**.
Usually every node communicates with about one index server. The index servers
then share the nodes information with each other. This means that the
information sent from a node to one index server should eventually reach all
other index servers that are reachable from that index server.

Index servers do not federate automatically with other index servers. Each
index server has a list of trusted index servers. For two index servers A and
B, only if the two conditions hold:

- A trusts B
- B trusts A

A and B will share node's information.

Every index server has a full picture of the whole nodes' funds network. This
allows index servers to find routes of wanted capacity efficiently, using
classical graph theoretic algorithms, like
[BFS](https://en.wikipedia.org/wiki/Breadth-first_search).

Anyone can run his own index server, but to have any value, this index server
must be part of the index servers federation. On his own, an index server will
only have partial information about the nodes' network, and therefore will not
be able to find routes to any place in the network.
