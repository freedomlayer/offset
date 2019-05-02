
# Proposal for atomic payments redesign

02.05.2019, real

## Abstract

This is a proposal for a new design for the core credit system for Offst,
allowing for atomic transactions.


## Current design

The current Offst design allows parties in a decentralized credit graph to push
payments along routes. In the current design, payments are guaranteed to
eventually resolve (To either success or failure). However, it is not known how
long it could take for payments to eventually resolve. 

We shortly describe the current Offst design:

Consider the following route:

```text
A --- B --- C --- D
```

Suppose that A wants to send credits to D along this route.
First a request message is sent from A all the way to D, freezing credits along
the way. 

```text
  -----[req]---->
A --- B --- C --- D
```

During the request period, if any node can not freeze enough credits with the next
node, the request will be cancelled by sending a backwards failure message:

```text
 ----[req]-->
 <--[fail]---
A --- B --- C --- D
```

In the figure above, C cancelled the payment request because not enough credits
were available to freeze between C and D.


If the request arrives at D successfuly, D will send a response message all the
way back to A. Every hop of the response message unfreezes credits and
completes a payment.

```text
 -------[req]----->
 <------[resp]-----
A --- B --- C --- D
```

In the figure above: A successful payment sent from A to D.

## Limitations of the current design

Consider the following route:

```text
A --- B --- C --- D
```

And imagine the payment experience from the point of view of the node A.
To pay D, A first issues a request message. After issuing this message, A has
no more control over the payment process. All A can do is wait for the
transaction to complete.

The request message sent from A might delay or get stuck in any hop from A to
D. A has no means of knowing what is the current position of the request
message, and has no ability to cancel that message. 

Theoretically at the request period it should be possible for A to cancel the payment
(Because no payment was actually done yet), however, A can only communicate
with his direct friends and therefore has no means of cancelling. In other
words, the current protocol design doesn't support it.

After the request message arrives at D (If it does arrive at D), a response
message leaves D back to A. This response message might also delay or get stuck
in any hop from D to A. At this stage it is not possible to cancel the payment,
because some funds were already transfered (First from C to D, then from B to C etc.)

If A is waiting at a store attempting to buy something, it is not acceptable
that A will wait more than a few seconds. We would like to have some way to
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
     /------------[response]------------
     | /----------[request]------------>
     | |
     | |   A ==== B ==== C ==== D ==== E
     | |   |
     | |   |
     | |   |     
     | |   | 
     | | +----+   Invoice   +----+
     | \-|    |<------------|    |
     |   | A' |   Receipt   | E' |
     \---|    |------------>|    |
         +----+             +----+
```

In the figure above one can see the process of payment from A to E.
A is the buyer's node and E is the seller's node. A' is the buyer client, and
E' is the seller's client. The buying process rephrased:

- E' sends an invoice to A'
- A' sends the invoice to A.
- A sends a request to E along a route.
- E sends a response back to A along the same route.
- A sends A' a receipt.
- A' forwards the receipt to E'.

It can be seen that in this process E' does not require any Internet
connectivity.


## Proposed design (Atomic transactions)



