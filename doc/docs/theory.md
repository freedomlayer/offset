# How Offset works

The offset protocol is a protocol that allows secure exchange of funds, based on
mutual trust.

## Mutual credit clearing

Assume that two business owners, a milkman and a baker, live next to each
other. Also assume that they live in a world without money.

One way the milkman and the baker could exchange goods without the use of money
is using the method of barter. the milkman could ask the baker from some bread,
in exchange for some milk. However, what if at a specific time the milkman
wants to get some bread from the baker, but the baker doesn't need any milk
from the milkman? In this case, the method of barter will not allow to perform
a transaction. Barter only works if the two involved parties are interested in
the exchange in a specific point in time.

To solve the problem, the milkman and the baker could use **mutual credit
clearing**: If the milkman wants to get bread from the baker, but the baker
doesn't need any milk right now, the baker will give the milkman a loaf of
bread, and write down on a piece of paper that the milkman owes him some
credit. The milkman will also write a similar note for reference.

A few days later later, if the baker wants to get milk from the milkman, the
baker could use his credit at the milkman to get milk. After the milkman gives
the baker a bottle of milk, the debt of the milkman to the baker is cleared.

This method allows the milkman and the baker to perform exchange of their
goods, even if they do not want to exchange them at exactly the same time.

One assumption that mutual credit clearing makes is that the milkman and the
baker trust each other. Suppose that the milkman takes 5 loafs of bread from
the baker. The milkman and the baker both write a note that the milkman ows the
baker credit that equals 5 loafs of bread. What if the milkman is gone one day,
without ever repaying the debt to the baker? The baker is left with a note that
the milkman ows him credit, but this note can not be redeemed anywhere for
goods.

The baker, by allowing the milkman to have debt to him, trusts that the milkman
will stick around to be able to pay back the debt in the future, in some way.

Some measures to be taken when using mutual credit clearing:

- Mutual credit clearing should only be set up between people that trust each
    other. It is advantage if those people know each other from the real world,
    and they have met in person.

- The maximum debt possible in the mutual credit clearing between two sides
    should be limited to some maximum amount. For example: If the baker allows the milkman to owe
    him credit of value at most 3 loafs of bread, the baker will not be able to lose
    more than 3 loafs of bread if A can't repay the debt.

Given these assumptions, the state of mutual credit between two nodes A and B
consists of the following parameters:

- A allows B maximum debt of md{AB}
- B allows A maximum debt of md{BA}

md{BA} and md{AB} are not necessarily equal. Drawn on a one dimensional scale,
we get a picture as follows (from the point of view of A):

```text
                     d{AB}
     [-----------|-----^-------]
  -md{BA}        0           md{AB}
```

In the picture, B owes A some amount of money. The `^` marks shows the current
credit relationship between A and B. The value d{AB} is the mutual credit state
between A and B. It denotes the purchasing power of A with respect to B. It is
always true that `-md{BA} <= d{AB} <= md{AB}`. Note the symmetric property:
`d{AB} = -d{BA}`.

From the point of view of B, the picture looks as follows:

```text
            d{BA)
     [-------^-----|-----------]
  -md{AB}          0         md{BA}
```

In the context of offset, we call such a pair of identities A and B
**friends**.  A pair of friends represents a trust relationship in the real
world between two people.

The economy of mutual credit clearing between two people is simple, and it
requires that each side generates goods that the other side wants. In the
example above, the milkman wants to get bread, and the baker wants to get milk.
This does not always happen in the real world.

If, for example, the baker decided to stop drinking milk (Maybe he got
allergic?), the simple mutual credit clearing between the milkman and the baker
will not keep working. After a few loafs of bread that the milkman takes from
the baker for credit, the baker will not allow the milkman to take any more,
because the debt is too large.

This problem could be solved if the baker possibly wanted to buy service from
the carpenter, assuming that the carpenter buys milk from the milkman. This
will usually happen in an economy of multiple players.

## Chains of mutual credit clearing

To extend the idea of credit clearing to the economy of multiple players we use
chains of mutual credit clearing. In this setting, every person has a few other
people he trusts, and maintains mutual credit clearing with.

Assume that a person A wants to send funds to another person B. If A and B
are friends (They directly manage mutual credit clearing), the transaction is
simple: A will decrease d{AB} and B will increase d{BA}.

If A and B are not friends, they need the help of a mediator, or a chain of
mediators, to perform the transaction. A and B will look for a chain of the
following form:

```text
A -- M1 -- M2 -- M3 -- B
```

Where A, M1 are friends, M1, M2 are friends, M2, M3 are friends and M3, B are
friends. Of course, the length of the chain could be arbitrary.

To transfer funds from A to B, A will first transfer funds to M1, M1 and will
transfer the funds to M2, M2 will transfer the funds to M3 and M3 will transfer
the funds to B. Each mediator (M1, M2, M3) can take a little amount of funds
for himself during the transaction, in exchange for helping A and B perform the
transaction.

We assume that friendship between people in the real world should allow any two
strangers A,B to find a chain of friends that connects them, to allow transfer
of funds between A and B. If no such chain is found, transfer of funds will not
be possible.

## Introduction to backwards credit payment

In the previous sections we showed that transfer of funds between
two nodes could be accomplished by "pushing" credit along a chain of friends.

To examine the security of this setup, we need to consider the incentives of
the nodes participating in the transaction. In this section we introduce a
method to perform a transaction between friends.

Consider the following graph of friends:

```text
A -- B -- C -- D
```

A,B are friends, B,C are friends, C,D are friends. Suppose that A wants to send
funds to D. We described earlier the general idea for sending funds
from A to B together with payments to the mediators, but we haven't yet gave
the specifics of how to do this, to make this transaction somewhat atomic and
secure.

Consider first the following naive scenario: A wants to send 10 credits to D.
A will calculate how much credit he needs to give to each of the mediators as a
payment for passing the funds all the way to D. For example: 2 credits to B, 2
credits to C and 10 credits to D.  Next, A will send 14 = 2 + 2 + 10 credits to B.
A trusts B to take 2 credits to himself, sending the rest of the credits to C.
In the same way, A could trust C to take 2 credits to himself, and pass the
remaining 10 credits to D.

B could keep 2 credits to himself, passing 12 credits to C. However, it is of
greater benefit to B to keep the full 14 credits to himself and not pass any
credits to C.

We solve this problem using the idea of **backwards payment of credits**. This idea
is crucial to the operation of the offset protocol.

When A wants to send funds to D, instead of directly passing all the 14 credits
to B, A will send B a promise for credit, in the future, given a proof that D
received the correct amount of credits. In addition, A freezes 14 credits in
his mutual credit with B. Those 14 credits will not be unfrozen until A gets a
proof from B that the funds were delivered to D, or until A gets a message from
B notifying that an error happened while processing the request.

Next, B passes a promise to C to pay 12 credits if C
brings a proof that the funds were delivered to D. B freezes 12 credits
in his mutual communication credit with C. C then sends a promise to D to pay
10 credits if D issues a signed message which proves D received the funds.

D receives the promise message, creates a signature of receipt, and sends it back to C.
C pays 10 credits to D. Next, C sends the signature back to B and receives 12
credits from B. B Finally B sends the signature back to A and receives 14
credits.

Eventually, A paid 14 credits, and B,C each earned 2 credits. D received 10
credits. In addition, A knows that the funds was received by D.

We distinguish between two stages in this transaction: We call the forward
stage (Sending the message from A to D) the `RequestSendFundsOp`, and the backwards stage
(Sending the signature from D to A) the `ResponseSendFundsOp`.

What happens if one of the mediators can not pass the message during the
request stage? For example, if C wants to pass the message to D, but C knows
that D is currently not online?
In this case, C will send back a `FailureSendFundsOp` message to B, claiming that the
funds could not be delivered, together with C's signature. B will pay C 1
credit. B will then add his own signature to the failure message, and forward
the failure message to A. Seeing the provided failure message, A will pay B 2
credits.

In other words, in the case of a failure, every mediator node (Up to the failure
reporting node) receives only 1 credit.

## Messages definition

We include here a more detailed description of each of the three messages used for
the transfer of funds between friends: `RequestSendFundsOp`, `ResponseSendFundsOp` and `FailureSendFundOp`.
(Comments begin with a hash (#) sign)

```capnp
struct RequestSendFundsOp {
        requestId @0: Uid;
        # A unique id of the request
        route @1: FriendsRoute;
        # A route of friends used to pass the credits. This route begins from the
        # party that initiates the payment, and goes all the way until the
        # destination, which receives the credits.
        destPayment @2: CustomUInt128;
        # Amount of credits that will be paid to the destination node.
        invoiceId @3: InvoiceId;
        # An invoice id number. This is used by the higher level application to
        # link the payment request to specific goods delivered.
}
```

```capnp
struct ResponseSendFundsOp {
        requestId @0: Uid;
        # The unique id of the original request.
        randNonce @1: RandNonce;
        # A random nonce.
        signature @2: Signature;
        # Signature{key=recipientKey}(
        #   sha512/256("FUND_SUCCESS") ||
        #   sha512/256(requestId || sha512/256(route) || randNonce) ||
        #   destPayment ||
        #   invoiceId
        # )
}
```

```capnp
struct FailureSendFundsOp {
        requestId @0: Uid;
        # The unique id of the original request.
        reportingPublicKey @1: PublicKey;
        # Index of the reporting node in the route of the corresponding request.
        # The reporting node cannot be the destination node.
        randNonce @2: RandNonce;
        # A random nonce added by the signer
        signature @3: Signature;
        # Signature{key=recipientKey}(
        #   sha512/256("FUND_FAILURE") ||
        #   requestId ||
        #   sha512/256(route) ||
        #   destPayment ||
        #   invoiceId ||
        #   reportingPublicKey ||
        #   randNonce
        # )
}
```

## Invoice and Receipt

We now consider higher level applications that use the credit clearing
mechanism for sending funds in exchange for some goods. There are two parties
in the transaction: The buyer and the seller.

```text
Buyer App                       Seller App
           <---[InvoiceId]----
           ----[Receipt]----->

```

Detailed process:

1. The buyer wants to buy something from the seller.
2. The seller generates a random invoice id and sends it to the buyer.
3. The buyer sends a `RequestSendFundsOp` along a route to the seller, containing
   the invoiceId provided by the seller.
4. The buyer receives a `ResponseSendFundsOp`, signed by the buyer. He uses it to
   construct a signed receipt that contains the invoiceId.
5. The buyer sends the receipt to the seller.
6. The seller verifies the receipt and provides the goods.

The structure of the receipt is as follows:

```capnp
struct Receipt {
        responseHash @0: Hash;
        # = sha512/256(requestId || sha512/256(route) || randNonce)
        invoiceId @1: InvoiceId;
        destPayment @2: CustomUInt128;
        signature @3: Signature;
        # Signature{key=recipientKey}(
        #   sha512/256("FUND_SUCCESS") ||
        #   sha512/256(requestId || sha512/256(route) || randNonce) ||
        #   invoiceId ||
        #   destPayment
        # )
}
```

Note that once a valid `ResponseSendFundsOp` message is received, it is possible to
construct a `Receipt`. The signature in the receipt is exactly the same signature
at the `ResponseSendFundsOp` message.

## Analyzing incentives in Backwards credit payment

We now analyze various cases of action during a backwards credit payment
transaction, to make sure that all the participants of the transaction are
properly incentivized. Our goal is that it will be the most profitable for
every participant to pass the funds to their destination (When possible).

Consider the following network formation between neighbors:

```text
A -- B -- C -- D -- E -- F
```

And consider the following cases:

**(1)** B receives a `RequestSendFundsOp` message that he should forward to C, but he
doesn't forward the message to C.

This is not a reasonable for B, because B could potentially earn credits
from this transaction.

**(2)** A sends a message to a nonexistent remote node T, along the route:

`A -- B -- C -- D -- T`

As a result, the node D will send back a `FailureSendFundsOp`
message, signed by D, back to C. The error response message will eventually
arrive A, and all the mediator nodes will be paid 1 credit. This means that
sending a message to a nonexistent remote node costs credit to A, and the
mediator nodes are still compensated.

**(3)** A and F cooperate. When F receives a `RequestSendFundsOp` message, F doesn't
return a signed response message to E. A and F might be using this as a
technique for free communication.

In this case E will keep the request open, waiting for a response from F. This
freezes credits between E and F. If F does this many times, all credits between
E and F will be frozen and it will not be possible to open new requests from E
to F. This means that F will not be able to receive funds through E.

**(4)** B receives a `ResponseSendFundsOp` message from C but doesn't pay C.

An inconsistency will be created in the neighbor relationship between B and C,
and communication will not continue between them until this inconsistency is
solved manually.

**(5)** C receives a `ResponseSendFundsOp` message from D but does not pass it to B.

This means that C gives up on credit, passing the `ResponseSendFundsOp` message to B
will earn C credit. Therefore, C will prefer to pass the `ResponseSendFundsOp`
message to B.

**(6)** An attacker node claims to be many nodes
For example, consider the following friends graph:

```text
A -- B -- C -- D -- E
     \---------/
       Attacker
```

Where B, C, D all belong to the same attacker. Those nodes possibly run on the
same machine, simulating multiple machines on the friends graph. It could be
very difficult for other nodes to notice that B, C, D run on the same machine.

Hence, whenever A sends funds to E through the route `A -- B -- C -- D -- E`,
the attacker obtains more credit compared to the honest friends graph:
`A - M - E`.

This is the equivalent of letting the attacker ask for more credit for
forwarding funds [^1]. Note however that if a route to a destination node is
long, it is likely that a different route will be chosen to send the funds.

Therefore the attacker can simulate a longer route only up to a certain length.
If the simulated route becomes too large, other shorter (and cheaper) routes
will be chosen instead.

[^1]:
    We might consider to add a feature for setting a transaction fee in future
    versions of offset. Currently the profit for mediating a transaction is
    fixed to be 1 credit.

Analyzing the cases above does not mean that the backwards credit payment is
proved to be safe, but currently we do not know of any holes in its design.

## Frozen credits

Assume the mutual credit relationship between two friends `A -- B`.
Consider the credit balance from the point of view of A.

We denote:

- balance is `d{AB}`, the balance between A and B from the point of view of A.
- remoteMaxDebt is `md{AB}`, the maximum debt A allows to B.
- localMaxDebt is `md{BA}`, the maximum debt B allows to A.
- remotePendingDebt is the current amount of credits frozen from B to A.
- localPendingDebt is the amount of credits frozen from A to B.

A picture of the credit balance from the point of view of A:

```text
      balance - localPendingD  balance       balance + remotePendingD
                      |          |                |
    ----------[-------(----------*----------------)------------]--------->
              |                                                |
    -localMaxDebt                                   remoteMaxDebt
```

We generally [^2] require the following inequalities:

```text
-localMaxDebt
    <= balance - localPendingDebt
    <= balance
    <= balance + remotePendingDebt
    <= remoteMaxDebt
```

[^2]:
    With the exception that one can configure remoteMaxDebt to any value, even
    lower than balance. In that case it should not be possible to increase
    balance.

## Credit Freezing DoS problem

Assume a mutual credit between two parties: `A -- B`.
Consider an attacker that has routes to A and routes to B:

```text
M -- .. -- A -- B -- .. -- N
```

In the picture above: M and N are nodes that are controlled by an attacker
(It is even possible that M and N are running on the same machine).

M sends a `RequestSendFundsOp` message to N through the route in the picture. N
receives the `RequestSendFundsOp` message and never sends a `ResponseSendFundsOp`
message.

If M keeps sending `RequestSendFundsOp` to which N never responds, all available
credits from A to B will be frozen. This will block any new `RequestSendFundsOp`
messages to be sent from A to B. (Note that this is not true for the other
direction! sending `RequestSendFundsOp` messages from B to A will still be
possible).

The attacker can perform this attack without losing significant credit. (If N
never sends a `ResponseSendFundsOp`, the attacker doesn't lose any credit). The
attacker does lose credit capacity around M and N, because M has to freeze at
least the same amount of credits A has to freeze.

However, if the attacker can obtain enough credit capacity (This should be
possible in most cases), the attacker might be able to block a specific
friendship channel between two parties.

## Sending funds may wait forever

When transferring large amount of credits in the graph of friends, the method
of indefinite waiting for response messages imposes risks for the sender of the
credits .

Consider the following graph of friends:

```text
A -- B -- C -- D
```

And consider the following cases:

(1) A wants to send funds to D. A sends a request message to
B. B sends a request message to C. Suddenly C crashes. As in the previous
example (in the graph of neighbors) is not known to B if C has received the
message, therefore B will wait indefinitely, until a response message is
received from C.

Meanwhile, A doesn't know if the transfer of funds to D has completed. If A has
sent a large amount of credit to D, this could be a problem. A could try to open
a new request to send funds to D, but then it is possible that the two
transactions will succeed, and eventually A sent D twice as much credit.

(2) A wants to send funds to D. A sends a request message to B, B forwards the
request message to C, C forwards the request message to D. D then sends back a
signed response message to C. This implies that C paid D the promised amount of credit.
C then suddenly crashes.

From D's point of view, the funds from A were received. A doesn't know if the
funds transfer transaction was completed.

**As a summary:**

- If A knows that the funds transfer was completed, then D has already received the funds.
- It is possible that D received the funds, but A doesn't know about this yet.

A possible solution to the lack of atomicity of funds transfer would be to
split large payments into smaller chunks. For example, if A wants to send D
1000 credits, A could send D 50 credits at a time. Most of the fund transfers
should complete successfully. If a transaction of 50 credits is somehow delayed,
A could attempt to send another 50 credits to D. At some point enough credits
have arrived to D.

Sending the total credits in smaller amounts might decrease the risk taken by
the sender.
