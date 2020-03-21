# Offset tutorial

## The core idea

Consider Alice, who has two friends, Bob and [Charli](https://en.wikipedia.org/wiki/Charli_XCX).
Suppose that at the same time:

- Bob owes Alice 50 dollars
- Alice also owes Charli 50 dollars.

```text
    (-50,+50)       (-50, +50)
Bob --------- Alice ---------- Charli

```

Suppose that Charli meets Bob one day, and Bob has a bicycle he wants to sell
for exactly 50 dollars. Suppose also that Charli is exactly looking to buy a
bicycle, and he decides to buy it from bob. In this case Charli can take the
bicycle, and they both ask Alice to forget about the debts on both sides.

In a sense, Charli made a payment to Bob by asking Alice to **offset** the debts.
No real money bills had to pass hands to make this happen.

```text
      (0, 0)          (0, 0)
Bob --------- Alice ---------- Charli

```

If Bob and Charli perform these kinds of transactions very often, it is
reasonable for Alice to charge for forwarding the transactions. For example,
Alice could charge 1 dollar for every transaction she forwards.

Offset is a system that works this way. People or organizations can set up
mutual credit with each other, and leverage those mutual credits to send
payments to anyone in the network. A participant that mediates a transaction
earns 1 credit.

## Initial setup

Offset is a network of nodes with mutual credit relationship between
them. Therefore the first thing we are going to do is to set up a node.
To be able to talk with our node we will also set up an application.

```text

    Node ----- Application

```

The Node is the part that does most of the hard work: processing transactions.
You can leave it to work in the background.

The Application is a way to communicate and control the node operation.
Some things that offset applications can do:

- Configure mutual credits with other nodes (Called: friends)
- View the current balance
- Find routes between nodes
- Send credits to a remote node

We start by creating a directory for our first experiment. Let's call it my_offset.

```bash
$ mkdir my_offset
$ cd my_offset
```

Next, we create a directory for our first node:

```bash
$ mkdir node0
# Trusted applications:
$ mkdir node0/trusted
```

And a directory for our first app:

```bash
$ mkdir app0
```

At this point, this is what your directory tree (inside `my_offset/`) should look like:

```text
my_offset/
├── app0
├── node0
│   └── trusted
```

All following commands will be run from the root directory (`my_offset/`).

### Cryptographic identity

Next, we create an cryptographic identity for our node and application. In
offset, every entity has a cryptographic identity which allows it to talk
securely with other entities.

```bash
$ stmgr gen-ident --output node0/node0.ident
$ stmgr gen-ident --output app0/app0.ident
```

### Node database

We initialize the node's database. The database contains the node's balances
with other nodes, and some other configuration.

```bash
$ stmgr init-node-db --idfile node0/node0.ident --output node0/node0.db
```

### Node ticket

Next, we create a ticket for the node. This serves an invitation for an
application to connect to the node:

```bash
$ stmgr node-ticket --address 127.0.0.1:9500 --idfile node0/node0.ident --output node0/node0.ticket
```

### Application ticket

An offset node is a program that manages your credits, so we can't let any
application connect to the node and perform operations. Therefore for every
application that we want to allow to connect to the node, we need to create a
ticket with specific permissions. Let's create a ticket for our application:

```bash
$ stmgr app-ticket --idfile app0/app0.ident --pconfig --pfunds --proutes --output node0/trusted/app0.ticket
```

The command above creates a ticket for app0 and stores it in the trusted dir of
node0. This will allow node0 to know that app0 is trusted.

Note the additional flags we used in the command: `--pconfig`, `--pfunds` and
`--proutes`. Those are permissions for configuration, sending funds and
requesting routes respectively.

### Starting the node

At this point you should have this file tree:

```text
├── app0
│   ├── app0.ident
│   └── node0.friend
├── node0
│   ├── node0.db
│   ├── node0.ident
│   ├── node0.ticket
│   └── trusted
│       └── app0.ticket
```

We can start the node with the command:

```bash
$ stnode --database node0/node0.db --idfile node0/node0.ident --laddr 127.0.0.1:9500 --trusted node0/trusted &
```

Note that the address we use for listening should be the same address as the
one advertised in the node ticket (See `stmgr node-ticket` above), otherwise
the application using the node ticket will connect to the wrong address.

The `&` at the end of the command means that the node will run in the background.

The node we have just spawned is "alone in the world". It does not have any
mutual credit with other nodes, and has no means of communication (because no
relay servers were configured) and no means of finding friend routes (no index servers
were configured). All that the node does now is wait for further commands from
an application.

### Connecting with stctrl

We will use the command line `stctrl` application to connect to communicate
with the node. To make sure that everything works correctly, run this command:

```bash
$ stctrl -I app0/app0.ident -T node0/node0.ticket info friends
```

If everything went well, the expected output is something of the form:

```text
No configured friends.
```

Which is true, because we have not yet configured any friends.

### Configuring relays

Relays are servers that help nodes communicate. Every node must have at least
one configured relay to function.

You can start your own relay, or use a public relay that someone else is running.

Configuring a relay is done using the command:

```bash
$ stctrl -I app0/app0.ident -T node0/node0.ticket config add-relay \
            -n my_relay -r my_relay.ticket
```

Where `-n my_relay` is your own name for this relay (You can pick any name you
want), and `-r my_relay.ticket` is a ticket file provided by the relay owner.

You can view the configured relays using stctrl's `info relays` subcommand.
Example:

```bash
$ stctrl -I app0/app0.ident -T node0/node0.ticket info relays
+------------+---------------------------------------------+----------------+
| relay name | public key                                  | address        |
+------------+---------------------------------------------+----------------+
| relay0     | Brvo3Fo0O2svzU1rFdcBL6FtLVNL6b8xJNAWgZc7ll0 | 127.0.0.1:8000 |
+------------+---------------------------------------------+----------------+
```

A relay can be removed using stctrl's `config remove-relay` subcommand.

### Index servers

Index servers are servers that help nodes find routes to send credits. Every
node must have at least one index server to be able to send and receive funds.

A connection to an index server allows a node to find routes to other nodes,
and at the same time allows other nodes to find routes to the node.

Anyone can start an index server, however, to be effective index servers
must federate with other index servers. This way information about nodes
can propagate.

Consider the following example: Node NA is connected to index server IA, and node NB is connected
to to index server IB:

```text
     IA                IB
     |                 |
     |                 |
     NA == ND == NE == NB

Legend:
| - Communication
= - Mutual credit relationship
```

Suppose that NA wants to send funds to NB. NA first asks the index server IA
for a route. If the index server IA does not federate with the index server IB,
IA will probably not know about the route `NA == ND == NE == NB`, and as a
result NA will not be able to send funds to NB.

### Configuring index servers

TODO: How to obtain public index servers?

An index server can be configured using the command:

```bash
$ stctrl -I app0/app0.ident -T node0/node0.ticket config add-index \
            -n my_index -i my_index.ticket
```

You can view the configured index servers using stctrl's `info index` subcommand. Example:

```bash
$ stctrl -I app0/app0.ident -T node0/node0.ticket info index
+-------------------+---------------------------------------------+----------------+
| index server name | public key                                  | address        |
+-------------------+---------------------------------------------+----------------+
| index0 (*)        | LMncLsod0HGDG66kEJJ3kki68CAGfjDsPTdGdLsyD5M | 127.0.0.1:9000 |
+-------------------+---------------------------------------------+----------------+
```

In this version of offset a node connects to only one index server at a
time. If the index server is down, an attempt is made to connect to the next
one on the list. (This behaviour might change in the future)

The currently connected index server is marked with a star ("*").

### Adding an extra node

To experiment with sending funds, we need at least one more node.
To get an extra node (Let's call it node1), we run the same commands but with
`node1` instead of `node0`. Also note that we use port `9501` instead of `9500` for
listening, as it is not possible for the two nodes to listen on the same TCP
port.

```bash
# Create directory tree:
$ mkdir node1
$ mkdir node1/trusted
$ mkdir app1

# Create identities:
$ stmgr gen-ident --output node1/node1.ident
$ stmgr gen-ident --output app1/app1.ident

# Prepare node:
$ stmgr init-node-db --idfile node1/node1.ident --output node1/node1.db
$ stmgr node-ticket --address 127.0.0.1:9501 --idfile node1/node1.ident --output node1/node1.ticket
$ stmgr app-ticket --idfile app1/app1.ident --pconfig --pfunds --proutes --output node1/trusted/app1.ticket

# Run node:
$ stnode --database node1/node1.db --idfile node1/node1.ident --laddr 127.0.0.1:9501 --trusted node1/trusted &

# Configure relay:
$ stctrl -I app1/app1.ident -T node1/node1.ticket config add-relay \
            -n my_relay -r my_relay.ticket

# Configure index:
$ stctrl -I app1/app1.ident -T node1/node1.ticket config add-index \
            -n my_index -i my_index.ticket
```

## Configuring mutual credit

At this point the two nodes (node0 and node1) do not know about each other:

```bash
$ stctrl -I app0/app0.ident -T node0/node0.ticket info friends
No configured friends.
$ stctrl -I app1/app1.ident -T node1/node1.ticket info friends
No configured friends.
```

### Exporting friend tickets

We create friend ticket for each node. This will allow the opposite node to add
it as a friend:

```bash
# Export node0 info as a friend file:
$ stctrl -I app0/app0.ident -T node0/node0.ticket info export-ticket -o app0/node0.friend
# Export node1 info as a friend file:
$ stctrl -I app1/app1.ident -T node1/node1.ticket info export-ticket -o app1/node1.friend
```

A friend file contains a node's public key and a list of relays that can be used to
communicate with a node.

```bash
$ cat app0/node0.friend
public_key = "TiTqXCEMBDoAyseEiw8t6r3L7do_k0iXOU1_rk4ERqw"

[[relays]]
public_key = "Brvo3Fo0O2svzU1rFdcBL6FtLVNL6b8xJNAWgZc7ll0"
address = "127.0.0.1:8000"
```

### Adding friends

```bash
# Node0 adds node1 as a friend:
$ stctrl -I app0/app0.ident -T node0/node0.ticket config add-friend -n node1 -f app1/node1.friend --balance=100
# Node1 adds node0 as a friend:
$ stctrl -I app1/app1.ident -T node1/node1.ticket config add-friend -n node0 -f app0/node0.friend --balance=-100
```

Note that in the above commands we chose that node0 has the initial balance of
100 credits, and node1 has the dual initial balance of -100.

If we now run `info friends` we get:

```bash
$ stctrl -I app0/app0.ident -T node0/node0.ticket info friends
+----+-------+---------------+
| st | name  | balance       |
+====+=======+===============+
| D- | node1 | C: LR=-, RR=- |
|    |       | B  =100       |
|    |       | LMD=0         |
|    |       | RMD=0         |
|    |       | LPD=0         |
|    |       | RPD=0         |
+----+-------+---------------+
```

We can see that the balance is 100 from node0's side.
Take a look at the `st` column: the status column. `D` means disabled, and `-` means offline.
The two nodes can not see each other, because we have not yet enabled
communication.

### Enabling friends communication

To enable communication, run:

```bash
# Node0: Enable communication to node1:
$ stctrl -I app0/app0.ident -T node0/node0.ticket config enable-friend -n node1
# Node1: Enable communication to node0:
$ stctrl -I app1/app1.ident -T node1/node1.ticket config enable-friend -n node0
```

Running `info friends` should now output:

```bash
$ stctrl -I app0/app0.ident -T node0/node0.ticket info friends
+----+-------+---------------+
| st | name  | balance       |
+====+=======+===============+
| E+ | node1 | C: LR=-, RR=- |
|    |       | B  =100       |
|    |       | LMD=0         |
|    |       | RMD=0         |
|    |       | LPD=0         |
|    |       | RPD=0         |
+----+-------+---------------+
```

Note that the status now is `E+`, which means enabled and online.

### Setting credit limit

We still can not send credits between node0 and node1, because we have not yet
configured the size of the credit limit. In other words, we need to configure:

- The maximum amount of credits node0 allows node1 to have in debt.
- The maximum amount of credits node1 allows node0 to have in debt.

The first amount can only be configured by node0, and the second amount can
only be configured by node1.

From the point of view of node0, the balance may only move in the limits:

`-localMaxDebt <= balance <= remoteMaxDebt`

Where `remoteMaxDebt` is chosen by node0, and `localMaxDebt` is chosen by node1:

```text
                               balance
                                 |
    ----------[------------------*-----------------------------]--------->
              |                                                |
    -localMaxDebt                                   remoteMaxDebt
```

Using the subcommand `info friends` we can see that the current `localMaxDebt`
(Denoted as LMD) and `remoteMaxDebt` (Denoted as RMD) are both 0. So this is
the current initial state from the point of view of node0:

```text
                      remoteMaxDebt   balance = 100
                              |       |
    --------------------------0-------*------------------>
                              |
                     -localMaxDebt
```

Note that this is a degenerated state, because balance is not between
`-localMaxDebt` and `remoteMaxDebt`. In this state it is only allowed for the
balance to go down to the direction of `remoteMaxDebt`.

Let's adjust the two max debt values. This can be done using the
`set-friend-max-debt` subcommand:

```bash
$ stctrl -I app1/app1.ident -T node1/node1.ticket config set-friend-max-debt -n node0 -m 150
$ stctrl -I app0/app0.ident -T node0/node0.ticket config set-friend-max-debt -n node1 -m 200
```

The new diagram from the point of view of node0 should now be:

```text
                                      balance = 100     remoteMaxDebt = 200
                                      |                |
    -------------[------------0-------*----------------]->
                 |
        -localMaxDebt = -150
```

### Opening requests

Consider the current output of the `info friends` subcommand:

```bash
$ stctrl -I app0/app0.ident -T node0/node0.ticket info friends
+----+-------+---------------+
| st | name  | balance       |
+====+=======+===============+
| E+ | node1 | C: LR=-, RR=- |
|    |       | B  =100       |
|    |       | LMD=150       |
|    |       | RMD=200       |
|    |       | LPD=0         |
|    |       | RPD=0         |
+----+-------+---------------+
```

The first line of the balance column contains `LR=-, RR=-`. `LR=-` means that
local requests are closed. In other words, it is not possible for node1 to open
payments requests through us. `RR=-` means that remote requests are closed:
node0 can not open payment requests through node1.

If we want to be able to send a payment from node0 to node1 we need to open the
requests at node1. This can be done using the `config open-friend` subcommand:

```bash
$ stctrl -I app0/app0.ident -T node0/node0.ticket config open-friend -n node1
$ stctrl -I app1/app1.ident -T node1/node1.ticket config open-friend -n node0
```

Requests are now open between node0 and node1:

```bash
$ stctrl -I app0/app0.ident -T node0/node0.ticket info friends
+----+-------+---------------+
| st | name  | balance       |
+====+=======+===============+
| E+ | node1 | C: LR=+, RR=+ |
|    |       | B  =100       |
|    |       | LMD=150       |
|    |       | RMD=200       |
|    |       | LPD=0         |
|    |       | RPD=0         |
+----+-------+---------------+
```

## Sending funds

There are currently two ways to send funds using stctrl:

- `send-funds`: Send funds without an invoice
- `pay-invoice`: Pay an invoice

Internally both commands work the same.
The difference between the two is that to pay with `pay-invoice` the recipient
party must first generate an invoice file (Specifying the payment amount).

On the other hand, `send-funds` allows raw sending of funds to any party given
that its public key is known. Paying with `send-funds` does not leave any means
for the recipient of the funds to relate them to any specific transaction.

### send-funds

Let's begin with `send-funds`, which is the raw method of sending funds:

We first observe the initial balance:

```bash
$ stctrl -I app0/app0.ident -T node0/node0.ticket info balance
0
```

Suppose that we want to send 50 credits from node0 to node1. We first need to
know node1's public key:

```bash
$ stctrl -I app1/app1.ident -T node1/node1.ticket info public-key
bUoWZEEInqjDdw8TOBlpY0zpHF7hjLMAX_DdPrTI9y8
```

Next, we use the `send-funds` subcommand to send credits:

```bash
$ stctrl -I app0/app0.ident -T node0/node0.ticket funds send-funds --amount 50 --dest bUoWZEEInqjDdw8TOBlpY0zpHF7hjLMAX_DdPrTI9y8
Payment successful!
Fees: 0
```

The new balance from the point of view of node0 and node1:

```bash
$ stctrl -I app0/app0.ident -T node0/node0.ticket info balance
-50
$ stctrl -I app1/app1.ident -T node1/node1.ticket info balance
50
```

### pay-invoice

Suppose that node1 wants to buy a bag of bananas from node0 that cost 60 credits.
To make the transaction, the following should happen:

1. node0 prepares an invoice for 60 credits and sends it to node1.
2. node1 pays the invoice and sends the receipt to node0.
3. node0 verifies the receipt and (if the receipt was valid) gives the bag of bananas to node1.

(1) **node0 prepares an invoice**

To prepare an invoice, we first get node0's public key:

```bash
$ stctrl -I app0/app0.ident -T node0/node0.ticket info public-key
TiTqXCEMBDoAyseEiw8t6r3L7do_k0iXOU1_rk4ERqw
```

node0 prepares an invoice using the `stregister` util:

```bash
$ stregister gen-invoice -a 60 -p TiTqXCEMBDoAyseEiw8t6r3L7do_k0iXOU1_rk4ERqw -o bananas.invoice
```

(2) **node1 pays the invoice**

node1 can now pay the invoice:

```bash
$ stctrl -I app1/app1.ident -T node1/node1.ticket funds pay-invoice -i bananas.invoice -r bananas.receipt
Payment successful!
Fees: 0
```

Note that a receipt file was created: bananas.receipt. The receipt file is a
proof that node1 paid the invoice successfully. Node1 now hands over the receipt
to node0.

(3) **node0 verifies the receipt**

```bash
$ stregister verify-receipt -i bananas.invoice -r bananas.receipt
Receipt is valid!
```

As expected, the balances now are:

```bash
$ stctrl -I app0/app0.ident -T node0/node0.ticket info balance
10
$ stctrl -I app1/app1.ident -T node1/node1.ticket info balance
-10
```

Now that the payment is verified, node0 can give node1 the bag of bananas.

## Running your own relay

Usually you will not need to run your own relay. You can configure your node to
use known public relays instead. If you still want to run your own relay, read
on.

We first need to create a new identity for the relay server. This can be done
as follows:

```bash
$ mkdir relay
$ stmgr gen-ident --output relay/relay.ident
```

Next, we can start the relay using this command:

```bash
strelay --idfile relay/relay.ident --laddr 127.0.0.1:8000 &
```

To allow nodes to connect to our relays, we need to provide a relay ticket.
A ticket can be generated using the following command:

```bash
$ stmgr relay-ticket --address 127.0.0.1:8000 --idfile relay/relay.ident --output relay/relay.ticket
```

Note that the address in the `stmgr relay-ticket` command must match the
address in the `strelay` command (Otherwise, nodes will connect to the wrong
relay address).

The ticket file `relay.ticket` can now be published. A user can download the
relay ticket file and apply it to a node using the command:

```bash
$ stctrl -I app0/app0.ident -T node0/node0.ticket config add-relay \
            -n my_relay -r relay.ticket
```

## Running your own index server

Usually you will not need to run your own index server. You can configure your
node to use known public index servers instead. If you still want to run your
own index server, read on.

We first need to create a new identity for the index server. This can be done
as follows:

```bash
$ mkdir index
$ stmgr gen-ident --output index/index.ident
```

Next, we need to set up a directory of trusted index servers:

```bash
mkdir index/trusted
```

And add a few trusted index servers tickets to this directory.

Note that it is required that the owners of those index servers will add our
index server as a trusted index server too. For communication to happen between
two index servers, it is crucial that both sides configure the remote side as a
trusted index server.

To generate an index server facing ticket, we run the command:

```bash
$ stmgr index-ticket --idfile index/index.ident --address 127.0.0.1:7000 \
        --output index_server.ticket
```

We will send the resulting `index_server.ticket` to the owner of the remote index servers we
want to communicate with.

To start the index server, we run:

```bash
stindex --idfile index/index.ident --lclient 127.0.0.1:9000 --lserver 127.0.0.1:7000 --trusted index/trusted &
```

We have two listening addresses above (lclient and lserver) because we listen
on two different TCP ports: One for incoming connections from nodes (lclient)
and one for incoming connections from federating index servers (lserver). Note
that the index server facing ticket we created earlier matches the `--lserver`
address.

To allow nodes to add our index server, we produce a node facing index ticket
as follows:

```bash
$ stmgr index-ticket --idfile index/index.ident --address 127.0.0.1:9000 \
        --output index_client.ticket
```

Note that this command is very similar to the command we used to produce an
index server facing ticket. The difference is the TCP port we have chosen and
the output file name.

Now we can publish the `index_client.ticket` file. A node can add the index
server to its configuration using this command:

```bash
$ stctrl -I app0/app0.ident -T node0/node0.ticket config add-index \
            -n my_index -i index_client.ticket
```
