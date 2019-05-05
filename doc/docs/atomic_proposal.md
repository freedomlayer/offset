
# Proposal for atomic payments redesign

05.05.2019, real

## Abstract

This is a proposal for a new design for the core credit system for Offst,
allowing for atomic transactions.


## Current design

The current Offst design allows parties in a decentralized credit graph to push
payments along routes. In the current design, payments are guaranteed to
eventually resolve (To either success or failure). However, it is not known how
long it could take for payments to eventually resolve. 

We shortly describe the current Offst design. To see a more detailed
explanation, see: [How offst works?](theory.md).

Consider the following route:

```text
B --- C --- D --- E
```

Suppose that B wants to send credits to E along this route.
First a request message is sent from B all the way to E, freezing credits along
the way. 

```text
  -----[req]---->
B --- C --- D --- E
```

During the request period, if any node can not freeze enough credits with the next
node, the request will be cancelled by sending a backwards failure message:

```text
 ----[req]-->
 <--[fail]---
B --- C --- D --- E
```

In the figure above, D cancelled the payment request because not enough credits
were available to freeze between D and E.


If the request arrives at E successfuly, E will send a response message all the
way back to B. Every hop of the response message unfreezes credits and
completes a payment.

```text
 -------[req]----->
 <------[resp]-----
B --- C --- D --- E
```

In the figure above: A successful payment sent from B to E.

## Limitations of the current design

Consider the following route:

```text
B --- C --- D --- E
```

And imagine the payment experience from the point of view of the node B.
To pay E, B first issues a request message. After issuing this message, B has
no more control over the payment process. All B can do is wait for the
transaction to complete.

The request message sent from B might delay or get stuck in any hop from B to
E. B has no means of knowing what is the current position of the request
message, and has no ability to cancel that message. 

Theoretically at the request period it should be possible for B to cancel the payment
(Because no payment was actually done yet), however, B can only communicate
with his direct friends and therefore has no means of cancelling. In other
words, the current protocol design doesn't support it.

After the request message arrives at E (If it does arrive at E), a response
message leaves E back to B. This response message might also delay or get stuck
in any hop from E to B. At this stage it is not possible to cancel the payment,
because some funds were already transfered (First from D to E, then from C to D etc.)

If B is waiting at a store attempting to buy something, it is not acceptable
that B will wait more than a few seconds. We would like to have some way to
send a payment atomically: Either the transaction occurs in a short period of time, or it
doesn't happen at all.

## Offline payment processing

The current design (two trips: request message and response message) has the
advantage of allowing the seller to be disconnected from the Internet (And from
the Offst network).

We describe a full buying story:

1. A buyer wants to buy an item.
2. The seller produces a random invoice value and hands it to the buyer.
3. The buyer sends a payment with the invoice provided by the seller (By
   sending a request message along a route)
4. The buyer receives a receipt (As a response message).
5. The buyer shows the receipt to the seller, and the seller can confirm the
   the payment is valid.
6. The seller hands the buyer the bought goods.


```text
     /---------[response]--------
     | /-------[request]-------->
     | |
     | |   B ==== C ==== D ==== E
     | |   |
     | |   |
     | |   |     
     | |   | 
     | | +----+    Invoice    +----+
     | \-|    |<--------------|    |
     |   | B' |    Receipt    | E' |
     \---|    |-------------->|    |
         +----+               +----+
```

In the figure above one can see the process of payment from B to E.
B is the buyer's node and E is the seller's node. B' is the buyer client, and
E' is the seller's client. The buying process:

- E' sends an invoice to B'
- B' sends the invoice to B.
- B sends a request to E along a route.
- E sends a response back to B along the same route.
- B sends B' a receipt.
- B' forwards the receipt to E'.

It can be seen that in this process the client E' does not need to be connected
to the node E (Hence no Internet connectivity is required).


## Proposed design outline (Atomic transactions)

Consider the following route:

```text
B --- C --- D --- E
```

Suppose that B wants to send credits to E along this route.


```text
Invoice        <=====[inv]========    (Out of band)

Request        ------[req]------->
Response       <-----[resp]-------

Confirmation   ======[conf]======>    (Out of band)
Goods          <=====[goods]======    (Out of band)

Commit         <-----[commit]-----
(Receipt)
               B --- C --- D --- E
```

The following operations occur:

1. E hands an Invoice to B. (Out of band)
2. B sends a Request message along a path to E.
3. E sends back a Response message along the same route to B.
4. B prepares a Confirmation message and hands it to E (Out of band).
5. E sends a Commit message along the route to B.
6. B receives the Commit message, prepares a Receipt and keeps it.


From the point of view of Offst users, this is how a transaction looks like:

```text
Invoice        <=====[inv]========    (Out of band)
Confirmation   ======[conf]======>    (Out of band)
Goods          <=====[goods]======    (Out of band)
               B -- ...   ... -- E
```

Note that the event of giving the confirmation by B is atomic.
The moment this event happens, the transaction is considered successful.
Note however, that it might take some time until the payment destination will
be able to collect his credits.

Cancellation can happen at any time after the Request message and before the
Commit message. A Cancellation message can only be sent by the payment
destination. 

### Examples for cancellation

- invoiceId is not recognized (by E):

```text
Invoice        <=====[inv]========    (Out of band)

Request        ------[req]------->
Cancel         <-----[cancel]-----

               B --- C --- D --- E
```


- Confirmation took too long to arrive:

```text
Invoice        <=====[inv]========    (Out of band)

Request        ------[req]------->
Response       <-----[resp]-------
Cancel         <-----[cancel]-----

               B --- C --- D --- E
```

## Messages definitions


### Request message

```text
 -------[req]----->
B --- C --- D --- E
```

This is the structure of the request message:

```capnp
struct RequestSendFundsOp {
        requestId @0: Uid;
        # Randomly generated reqeustId [128 bits]
        srcHashedLock @1: Hash;
        # sha512/256(srcPlainLock), where srcPlainLock is of size 256 bits.
        route @2: FriendsRoute;
        # A route of friends that leads to the destination
        destPayment @3: CustomUInt128;
        # Amount of credits to pay the destination
        invoiceId @4: InvoiceId;
        # A 256 bit value representing the invoice this request
        # intends to pay.
}
```

The request message mainly verifies that there is enough capacity to make the
payment along the route (Including capacity for the transaction fees). 
For example, if B wants to send 10 credits to E, then during the request
message passage from B to E:

- B checks that B -> C has at least the capacity of 12 credits.
- C checks that C -> D has at least the capacity of 11 credits.
- D checks that D -> E has at least the capacity of 10 credits.

The extra credits are due to transaction fees to C and D (1 credit each).


### Response message

If all went well, E sends back a **response** message along the
same path, all the way back to B.


```text
 <------[resp]-----
B --- C --- D --- E
```

```capnp
struct ResponseSendFundsOp {
        requestId @0: Uid;
        destHashedLock @1: Lock;
        randNonce @2: RandNonce;
        signature @3: Signature;
        # Signature{key=destinationKey}(
        #   sha512/256("FUNDS_RESPONSE") ||
        #   sha512/256(requestId || sha512/256(route) || randNonce) ||
        #   srcHashedLock || 
        #   receiptHashedLock ||
        #   destPayment ||
        #   invoiceId
        # )
        #
        # Note that the signature contains an inner blob (requestId || ...).
        # This is done to make the size of the receipt shorter.
        # See also the Receipt structure.
}
```

### Cancel message

If any node could not forward the request message, or the destination decided
to cancel the transaction, a failure message will be sent back, beginning from
the failing node.

A cancel message can be sent as long as **receipt message** was not yet sent.

```text
 <----[cancel]-----
B --- C --- D --- E
```

```capnp
struct CancelSendFundsOp {
        requestId @0: Uid;
        reportingPublicKey @1: PublicKey;
        # Public key of the reporting node in the route of the corresponding request.
        # The reporting node cannot be the destination node.
        randNonce @2: RandNonce;
        signature @3: Signature;
        # Signature{key=recipientKey}(
        #   sha512/256("FUNDS_CANCEL") ||
        #   requestId ||
        #   srcHashedLock ||
        #   sha512/256(route) ||
        #   destPayment ||
        #   invoiceId ||
        #   reportingPublicKey ||
        #   randNonce
        # )
}
```

### Confirmation message (Out of band)

After receiving a Response message, the source node of the payment creates a
Confirmation message. The Confirmation message is given to the destination, and
at that moment the payment is considered successful. 

```text
=======[conf]=====>  (Out of band)
B --- C --- D --- E
```

Verification of the Confirmation message is done by checking the following:

- invoiceId matches the originally issued invoice.
- destPayment matches the wanted amount of credits.
- The signature is valid (Signed by the destination of the payment)
- The revealed lock is valid: `sha512/256(srcPlainLock) == srcHashedLock`


Upon receipt the Confirmation message, the destination will give the goods to
the buyer, and send back (along the same route) a Commit message to collect his
credits.

```capnp
struct Confirmation {
        responseHash @0: Hash;
        # = sha512/256(requestId || sha512/256(route) || randNonce)
        invoiceId @1: InvoiceId;
        destPayment @2: CustomUInt128;
        srcPlainLock: @3: Lock;
        # The preimage of the hashedLock at the request message [256 bits]
        signature @4: Signature;
        # Signature{key=destinationKey}(
        #   sha512/256("FUNDS_RESPONSE") ||
        #   sha512/256(requestId || sha512/256(route) || randNonce) ||
        #   srcHashedLock || 
        #   dstHashedLock || 
        #   receiptHashedLock ||
        #   destPayment ||
        #   invoiceId
        # )
}
```


### Commit message

After receiving a confirmation message from the payment source, the destination
gives the goods to the payment source and sends back a Commit message to
collect his credits.

```text
 <----[commit]-----
B --- C --- D --- E
```


A Commit message completes the transaction. For example, when the Commit
message is sent from E to D, the credits that were frozen between D and E
become unfrozen, and the payment is irreversible. The Commit messages continues
all the way (along the original route) to the source of the payment.


```capnp
struct CommitOp {
        requestId @0: Uid;
        srcPlainLock @1: Lock;
        destPlainLock @2: Lock;
}
```

Note that the Commit message can only be sent by the payment destination after
it has received the confirmation, because the confirmation contains the
srcPlainLock.

When receiving a CommitOp messages, the following should be verified:

- `sha512/256(srcPlainLock) = srcHashedLock`
- `sha512/256(destPlainLock) = destHashedLock`


### Receipt

Upon receiving the Receipt message, the source of the payment can compose a
Receipt.

```capnp
struct Receipt {
        responseHash @0: Hash;
        # = sha512/256(requestId || sha512/256(route) || randNonce)
        invoiceId @1: InvoiceId;
        srcPlainLock @2: Lock;
        destPlainLock @3: Lock;
        signature @4: Signature;
        # Signature{key=destinationKey}(
        #   sha512/256("FUNDS_RESPONSE") ||
        #   sha512/256(requestId || sha512/256(route) || randNonce) ||
        #   srcHashedLock || 
        #   dstHashedLock || 
        #   destPayment ||
        #   invoiceId
        # )
}
```

The Receipt can be constructed only after the CommitOp message was received.
Note that it is possible that a receipt can be constructed only a long time
after the confirmation message was given.


## Atomicity

Assume that the node E issued an invoice and handed it to B.

B wants to pay the invoice. The payment begins by sending a Request message
along the path from B to E. The payment is considered successful when B hands a
Confirmation message to E. 

This means that we should examine the possiblity of B waiting indefinitely
during the sending of Request and Response messages along the route.

During this time (Request + Response period), B can discard the transaction by
walking away. E will not be able to make progress because in order to send the
Commit message, the correct srcPlainLock is required, but E does not know it
before B sends the Confirmation message.

Also note that if B sends a valid Confirmation message to E, the transaction is
considered successful, and B can not reverse it. This happens because B reveals
srcPlainLock at the Confirmation message sent to E.


## Cancellation responsibility

Only the payment destination can issue a Cancel message (Sent from the
destination along a path to the source). A Cancel message will be sent by the
Offst node automatically for any incoming Request message that contains a non
recognized InvoiceId.

In addition, cancellation can be issued for a certain `invoiceId` from the
application level. Cancellation message should only be sent after the invoice was
issued and before a Confirmation message was received. It might be possible for
applications to cancel `invoiceId`-s after a certain amount of time.

- Sending a Cancel message after a Confirmation message was received is
    considered a bad form for the seller (payment destination), and can be seen
    as equivalent to not delivering the goods after a successful payment. 

- Sending a Cancel
    message after the goods were given to the payment source will cause the
    seller to lose credits.


The only way for the buyer (payment source) to cancel a transaction is by never
sending a Confirmation message to the seller (payment destination).


## Extra: Payment without Confirmation

Consider the following route:

```text
B --- C --- D --- E
```

Suppose that B wants to send credits to E without any atomicity guarantees. 

For example, B might want to send a donation to E. Therefore B does not care
about the atomicty of the transaction.

The resulting protocol is as follows:

```text
Invoice        <=====[inv]========    (Out of band)

Request        ------[req]------->
Response       <-----[resp]-------

Commit         <-----[commit]-----
(Receipt)
               B --- C --- D --- E
```

(Note that the Out of band Invoice part is optional).

This can be achieved by having B choose a well known value for `srcPlainLock`,
for example, 256 consecutive zero bytes (`'\x00' * 256`). This should allow E
to "guess" `srcPlainLock` and send back a Commit message immediately.
Therefore the out of band Confirmation message sent from B is not required.




