Proposal for atomic payments redesign
=====================================

14.05.2019, real

Abstract
--------

This is a proposal for a new design for the core credit system for
Offset, allowing for atomic transactions.

Current design
--------------

The current Offset design allows parties in a decentralized credit graph
to push payments along routes. In the current design, payments are
guaranteed to eventually resolve (To either success or failure).
However, it is not known how long it could take for payments to
eventually resolve.

We shortly describe the current Offset design. To see a more detailed
explanation, see: `"How offset works?" <theory.md>`__.

Consider the following route:

.. code:: text

    B --- C --- D --- E

Suppose that B wants to send credits to E along this route. First a
request message is sent from B all the way to E, freezing credits along
the way.

.. code:: text

      -----[req]---->
    B --- C --- D --- E

During the request period, if any node can not freeze enough credits
with the next node, the request will be cancelled by sending a backwards
failure message:

.. code:: text

     ----[req]-->
     <--[fail]---
    B --- C --- D --- E

In the figure above, D cancelled the payment request because not enough
credits were available to freeze between D and E.

If the request arrives at E successfully, E will send a response message
all the way back to B. Every hop of the response message unfreezes
credits and completes a payment.

.. code:: text

     -------[req]----->
     <------[resp]-----
    B --- C --- D --- E

In the figure above: A successful payment sent from B to E.

Limitations of the current design
---------------------------------

Consider the following route:

.. code:: text

    B --- C --- D --- E

And imagine the payment experience from the point of view of the node B.
To pay E, B first issues a request message. After issuing this message,
B has no more control over the payment process. All B can do is wait for
the transaction to complete.

The request message sent from B might delay or get stuck in any hop from
B to E. B has no means of knowing what is the current position of the
request message, and has no ability to cancel that message.

Theoretically at the request period it should be possible for B to
cancel the payment (Because no payment was actually done yet), however,
B can only communicate with his direct friends and therefore has no
means of cancelling. In other words, the current protocol design doesn't
support it.

After the request message arrives at E (If it does arrive at E), a
response message leaves E back to B. This response message might also
delay or get stuck in any hop from E to B. At this stage it is not
possible to cancel the payment, because some funds were already
transferred (First from D to E, then from C to D etc.)

If B is waiting at a store attempting to buy something, it is not
acceptable that B will wait more than a few seconds. We would like to
have some way to send a payment atomically: Either the transaction
occurs in a short period of time, or it doesn't happen at all.

Offline payment processing
--------------------------

The current design (two trips: request message and response message) has
the advantage of allowing the seller to be disconnected from the
Internet (And from the Offset network).

We describe a full buying story:

1. A buyer wants to buy an item.
2. The seller produces a random invoice value and hands it to the buyer.
3. The buyer sends a payment with the invoice provided by the seller (By
   sending a request message along a route)
4. The buyer receives a receipt (As a response message).
5. The buyer shows the receipt to the seller, and the seller can confirm
   the payment is valid.
6. The seller hands the buyer the bought goods.

.. code:: text

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

In the figure above one can see the process of payment from B to E. B is
the buyer's node and E is the seller's node. B' is the buyer client, and
E' is the seller's client. The buying process:

-  E' sends an invoice to B'
-  B' sends the invoice to B.
-  B sends a request to E along a route.
-  E sends a response back to B along the same route.
-  B sends B' a receipt.
-  B' forwards the receipt to E'.

It can be seen that in this process the client E' does not need to be
connected to the node E (Hence no Internet connectivity is required).

Proposed design outline (Atomic transactions)
---------------------------------------------

Consider the following route:

.. code:: text

    B --- C --- D --- E

Suppose that B wants to send credits to E along this route. We call B
the **buyer**, and E the **seller**.

.. code:: text

    Invoice        <=====[inv]========    (Out of band)

    Request        ------[req]------->
    Response       <-----[resp]-------

    Commit         ======[commit]====>    (Out of band)
    Goods          <=====[goods]======    (Out of band)

    Collect        <-----[collect]----
    (Receipt)
                   B --- C --- D --- E

The following operations occur:

1. E hands an Invoice to B. (Out of band)
2. B sends a Request message along a path to E.
3. E sends back a Response message along the same route to B.
4. B prepares a Commit message and hands it to E (Out of band).
5. E sends a Collect message along the route to B.
6. B receives the Collect message, prepares a Receipt and keeps it.

From the point of view of Offset users, this is how a transaction looks
like:

.. code:: text

    Invoice        <=====[inv]========    (Out of band)
    Commit         ======[commit]====>    (Out of band)
    Goods          <=====[goods]======    (Out of band)
                   B -- ...   ... -- E

Note that the event of B handing the confirmation to E is atomic. The
moment this event happens, the transaction is considered successful.
Note however, that it might take some time until the seller E will be
able to collect his credits.

Cancellation can happen at any time after the Request message was sent
from the buyer and before the Collect message was sent.

During the Request stage a cancellation message could be sent from any
node forwarding the Request message. However, after the Request message
arrives at the seller node, only the seller node may issue a
cancellation message. (This rule has one exception that happens during
unfriending, see below).

Examples for cancellation
~~~~~~~~~~~~~~~~~~~~~~~~~

-  An intermediate node cancels the transaction during the Request
   period. This can happen for example if there is not enough capacity
   for pushing credits forward.

.. code:: text

    Invoice        <=====[inv]========    (Out of band)

    Request        ---[req]---->
    Cancel         <--[cancel]--

                   B --- C --- D --- E

-  invoiceId is not recognized (by E):

.. code:: text

    Invoice        <=====[inv]========    (Out of band)

    Request        ------[req]------->
    Cancel         <-----[cancel]-----

                   B --- C --- D --- E

-  Commit took too long to arrive:

.. code:: text

    Invoice        <=====[inv]========    (Out of band)

    Request        ------[req]------->
    Response       <-----[resp]-------
    Cancel         <-----[cancel]-----

                   B --- C --- D --- E

-  Cancellation in Request period that happens due to unfriending nodes:

.. code:: text

    Invoice        <=====[inv]========    (Out of band)

    Request        ------[req]------->
                   B --- C --- D --- E
    Unfriend
    Cancel         <--[cancel]--
                   B --- C --- D     E

In the figure above: a request was sent from B to E. Next, D unfriends E
before E manages to send the response message. In that case D sends a
Cancel message for this transaction all the way back to B, and the
transaction credits are unfrozen.

-  Cancellation in Response period that happens due to unfriending
   nodes:

.. code:: text

    Invoice        <=====[inv]========    (Out of band)

    Request        ------[req]------->
    Response       <-----[resp]-------
                   B --- C --- D --- E
    Unfriend
    Cancel         <--[cancel]--
                   B --- C --- D     E

Messages definitions
--------------------

(\*) Request message
~~~~~~~~~~~~~~~~~~~~

.. code:: text

     -------[req]----->
    B --- C --- D --- E

This is the structure of the request message:

.. code:: capnp

    struct RequestSendFundsOp {
            requestId @0: Uid;
            # Randomly generated reqeustId [128 bits]
            srcHashedLock @1: HashedLock;
            # bcrypt(srcPlainLock), where srcPlainLock is of size 256 bits.
            route @2: FriendsRoute;
            # A route of friends that leads to the destination
            destPayment @3: CustomUInt128;
            # Amount of credits to pay the destination over this route.
            totalDestPayment @4: CustomUInt128;
            # Total amount of credits to be paid (Must match the invoice)
            # totalDestPayment > destPayment in cases of multi route-payments.
            invoiceId @4: InvoiceId;
            # A 256 bit value representing the invoice this request
            # intends to pay.
    }

The request message mainly verifies that there is enough capacity to
make the payment along the route (Including capacity for the transaction
fees). For example, if B wants to send 10 credits to E, then during the
request message passage from B to E:

-  B checks that B -> C has at least the capacity of 12 credits.
-  C checks that C -> D has at least the capacity of 11 credits.
-  D checks that D -> E has at least the capacity of 10 credits.

The extra credits are due to transaction fees to C and D (1 credit
each).

The Request message contains a hash lock: ``srcHashedLock``. This value
is generated by the buyer by generating a random ``srcPlainLock`` value
and hashing it: ``srcHashedLock := bcrypt(srcPlainLock)``. This
mechanism is used to ensure transaction atomicity: The seller can not
create a valid Collect message without knowing the secret value
``srcPlainLock``.

(\*) Response message
~~~~~~~~~~~~~~~~~~~~~

If all went well during the Request stage, E sends back a Response
message along the same path, all the way back to B.

.. code:: text

     <------[resp]-----
    B --- C --- D --- E

.. code:: capnp

    struct ResponseSendFundsOp {
            requestId @0: Uid;
            destHashedLock @1: HashedLock;
            randNonce @2: RandNonce;
            signature @3: Signature;
            # Signature{key=destinationKey}(
            #   sha512/256("FUNDS_RESPONSE") ||
            #   sha512/256(requestId || randNonce) ||
            #   srcHashedLock ||
            #   destHashedLock ||
            #   destPayment ||
            #   totalDestPayment ||
            #   invoiceId
            # )
            #
            # Note that the signature contains an inner blob (requestId || ...).
            # This was done to make the size of the receipt shorter, as previously
            # this contained a full route.
    }

Note that the response contains a ``destHashedLock``. This value is
created by hashing a secret generated by the seller:
``destHashedLock := bcrypt(destPlainLock)``. This secret will only be
revealed when the Collect message is sent. We have this mechanism to
defend against fake Receipt generated by the buyer. (A valid receipt
must contain the secret ``destPlainLock``).

(\*) Cancel message
~~~~~~~~~~~~~~~~~~~

A Cancel message may be sent back by any node during the Request period.
After Request message arrived at the seller node and before the Collect
message was sent, only the seller node may send a Cancel message. In
addition, any node may send a Cancel message to cancel ongoing
transactions in case of unfriending a node (As long as the Collect
message was not yet received).

After the Collect message was received, the transaction can not be
cancelled.

If any node could not forward the request message, or the destination
decided to cancel the transaction, a failure message will be sent back,
beginning from the failing node.

A cancel message can be sent as long as **receipt message** was not yet
sent.

.. code:: text

     <----[cancel]-----
    B --- C --- D --- E

.. code:: capnp

    struct CancelSendFundsOp {
            requestId @0: Uid;
    }

(\*) Commit message (Out of band)
~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

After receiving a Response message, the source node of the payment
creates a Commit message. The Commit message is given to the
destination, and at that moment the payment is considered successful.

.. code:: text

    ======[commit]====>    (Out of band)
    B --- C --- D --- E

Upon receipt of a valid Commit message, the seller will give the goods
to the buyer, and send back (along the same route) a Collect message to
collect his credits.

.. code:: capnp

    struct Commit {
            responseHash @0: Hash;
            # = sha512/256(requestId || randNonce)
            destPayment @1: CustomUInt128;
            # Amount of credits paid in this Transaction.
            srcPlainLock @2: PlainLock;
            # The preimage of the hashedLock at the request message [256 bits]
            destHashedLock @3: HashedLock;
            signature @4: Signature;
            # Signature{key=destinationKey}(
            #   sha512/256("FUNDS_RESPONSE") ||
            #   sha512/256(requestId || sha512/256(route) || randNonce) ||
            #   srcHashedLock || 
            #   destHashedLock || 
            #   destPayment ||
            #   totalDestPayment ||
            #   invoiceId
            # )
    }

    struct MultiCommit {
            invoiceId @0: InvoiceId;
            # InvoiceId being paid.
            totalDestPayment @1: CustomUInt128;
            # The total amount being paid
            commits @2: List(Commit);
            # A list of confirmations. Each confirmation corresponds to a request
            # sent along one route.
    }

Note that the ``MultiCommit`` message may contain multiple ``Commit``-s,
each corresponding to one request. This allows fragmented payment along
multiple routes.

Verification of a MultiCommit message by the seller is done as follows:

-  InvoiceId matches an originally issued invoice.
-  For every Commit:

   -  The revealed lock is valid:
      ``bcrypt(srcPlainLock) == srcHashedLock``
   -  signature is valid.

-  Total of ``destPayment`` is correct (Equal the requested amount at
   the invoice).

(\*) Collect message
~~~~~~~~~~~~~~~~~~~~

After receiving a confirmation message from the buyer, the destination
gives the goods to the buyer and sends back a Collect message to collect
his credits.

.. code:: text

     <----[collect]----
    B --- C --- D --- E

A Collect message completes the transaction. For example, when the
Collect message is sent from E to D, the credits that were frozen
between D and E become unfrozen, and the payment is irreversible. The
Collect messages continues all the way (along the original route) to the
source of the payment.

.. code:: capnp

    struct CollectSendFundsOp {
            requestId @0: Uid;
            srcPlainLock @1: PlainLock;
            destPlainLock @2: PlainLock;
    }

Note that the Collect message can only be sent by the seller after it
has received the confirmation, because the confirmation contains the
srcPlainLock.

When receiving a CollectSendFundsOp messages, the following should be
verified:

-  ``bcrypt(srcPlainLock) = srcHashedLock``
-  ``bcrypt(destPlainLock) = destHashedLock``

(\*) Receipt
~~~~~~~~~~~~

Upon receiving the Receipt message, the source of the payment can
compose a Receipt.

.. code:: capnp

    struct Receipt {
            responseHash @0: Hash;
            # = sha512/256(requestId || randNonce)
            invoiceId @1: InvoiceId;
            srcPlainLock @2: PlainLock;
            destPlainLock @3: PlainLock;
            destPayment @4: CustomUInt128;
            totalDestPayment @4: CustomUInt128;
            signature @5: Signature;
            # Signature{key=destinationKey}(
            #   sha512/256("FUNDS_RESPONSE") ||
            #   sha512/256(requestId || sha512/256(route) || randNonce) ||
            #   srcHashedLock || 
            #   dstHashedLock || 
            #   destPayment ||
            #   totalDestPayment ||
            #   invoiceId
            # )
    }

The Receipt can be constructed only after the CollectSendFundsOp message
was received. Note that it is possible that a receipt can be constructed
only a long time after the confirmation message was given.

Atomicity
---------

Atomicity is guaranteed by using a `hash
lock <https://en.bitcoin.it/wiki/Hashlock>`__ created by the buyer:
``srcHashedLock``.

Assume that the node E issued an invoice and handed it to B.

B wants to pay the invoice. The payment begins by sending a Request
message along the path from B to E. The payment is considered successful
when B hands a MultiCommit message to E.

This means that we should examine the possibility of B waiting
indefinitely during the sending of Request and Response messages along
the route.

During this time (Request + Response period), B can discard the
transaction by walking away. E will not be able to make progress because
in order to send the Collect message, the correct srcPlainLock is
required, but E does not know it before B sends the MultiCommit message.

Also note that if B sends a valid MultiCommit message to E, the
transaction is considered successful, and B can not reverse it. This
happens because B reveals srcPlainLock at the MultiCommit message sent
to E.

Receipt verifiability
---------------------

A receipt is a proof that a certain invoice was paid. It can be verified
by anyone that possesses:

-  The invoice (``invoiceId`` + public key of seller)
-  The Receipt

Verification is performed by checking the signature (See description of
signature at the Receipt definition).

In order to make sure the buyer can not have a valid Receipt before the
payment actually completed, we use a hash lock that is issued by the
payment destination: ``destHashedLock``.

When the buyer receives a Response message it can not yet create a valid
Receipt, because the buyer doesn't yet know ``destPlainLock``. This
value is revealed only at the Collect message, when the payment is
considered to be successful.

Note: An alternative solution could be to let the seller sign a new
signature over the Collect message, but instead we chose to use a hash
lock, which is a less expensive cryptographic operation. Using a hash
lock also does not require access to the identity of the seller.

This leaves the whole protocol with only one cryptographic signature
over the Response message, signed by the seller.

Cancellation responsibility
---------------------------

Only the seller can issue a Cancel message (Sent from the destination
along a path to the source). A Cancel message will be sent by the Offset
node automatically for any incoming Request message that contains a non
recognized InvoiceId (TODO: Can this cause any issues?)

In addition, cancellation can be issued for a certain ``invoiceId`` from
the application level. Cancellation message should only be sent after
the invoice was issued and before a Commit message was received. It
might be possible for applications to cancel ``invoiceId``-s after a
certain amount of time.

-  Sending a Cancel message after a Commit message was received is
   considered a bad form for the seller, and can be seen as equivalent
   to not delivering the goods after a successful payment.

-  Sending a Cancel message after the goods were given to the buyer will
   cause the seller to lose credits.

The only way for the buyer to cancel a transaction is by never sending a
Commit message to the seller.

Fragmented/Multi-Route transactions
-----------------------------------

Sometimes it might not be possible to send a payment along a single
route. In such cases it is useful to send the transaction along multipe
routes. The protocol allows sending a payment along multiple routes
atomically. This is done as follows:

1. Buyer gets an Invoice from the seller for a certain amount of
   credits.
2. Buyer sends a RequestSendFundsOp along a route.
3. A ResponseSendFundsOp or a CancelSendFundsOp message is returned.
4. Go back to (2) until the wanted amount of credits is acheived (for
   paying the invoice).
5. Buyer sends a MultiCommit message containing a list of all Commit-s
   for all the requests that got a valid response.
6. Seller verifies the MultiCommit message. If valid, the payment is
   accepted and the goods are handed to the buyer.
7. The Seller sends back CollectSendFundsOp messages for all requests.
8. Any CollectSendFundsOp message can be used to create a valid Receipt.
   (Two diferent constructed receipts will have the same invoiceId but
   different responseHash).

Extra: Non-atomic payment without Commit
----------------------------------------

Consider the following route:

.. code:: text

    B --- C --- D --- E

Suppose that B wants to send credits to E without any atomicity
guarantees.

For example, B might want to send a donation to E. Therefore B does not
care about the atomicity of the transaction.

The resulting protocol is as follows:

.. code:: text

    Invoice        <=====[inv]========    (Out of band)

    Request        ------[req]------->
    Response       <-----[resp]-------

    Collect        <-----[collect]----
    (Receipt)
                   B --- C --- D --- E

(Note that the Out of band Invoice part is optional).

This can be achieved by having B choose a well known value for
``srcPlainLock``, for example, 256 consecutive zero bytes
(``'\x00' * 256``). This should allow E to "guess" ``srcPlainLock`` and
send back a Collect message immediately. Therefore the out of band
Commit message sent from B is not required.

This construction allows taking donations through a static HTTPS
webpage. All one has to do is publish his public key. We are still not
though sure if adding this guessing feature will have real world uses.

Idea: Change ``srcHashedLock`` to be a sum type of None or
srcHashedLock? This way guessing will not be required. Might depend on
the hash and salt representation.

Note: We can not use this for multi-route payments.

Application interface
---------------------

We describe here Offset nodes' interface with an Application. (Unrelated
interface messages are not mentioned here for clarity).

The interface is split here between Buyer and Seller for clarity,
however note that there is only one Node implementation, and both the
Buyer and the Seller use this implementation.

Buyer interface
~~~~~~~~~~~~~~~

.. code:: text

    CreatePayment [App -> Node]
    =============
    - paymentId
    - invoiceId
    - totalDestPayment
    - Destination public key

    CreateTransaction [App -> Node]
    =================
    - paymentId
    - requestId
    - route
    - fees
    - destPayment

    TransactionResult [Node -> App]
    =================
    - Response or Cancellation

    <!--
    SendFunds [App -> Node]
    =========
    (Extra: non atomic payment form)
    - Destination public key
    - Amount of credits
    -->

    RequestReceipt [App -> Node]
    ==============
    - paymentId


    ResponseReceipt [Node -> App]
    ===============
    - Receipt or Empty


    RemoveReceipt [App -> Node]
    =============
    - paymentId

To pay an invoice, a buyer can do the following:

-  Send ``RequestPay(invoice)`` to the Node.
-  Wait for ``ResponsePay`` to arrive from the Node. If waiting takes
   too long, forget about the payment.
-  If ``ResponsePay`` contains cancel, cancel the payment. Else, send
   Commit to the seller.

After a while:

-  Send ``RequestReceipt`` to the Node.
-  If ResponseReceipt is Empty, try again at a later time. Else, keep
   the receipt in persistent storage and send ``RemoveReceipt`` to the
   Node.

Seller interface
~~~~~~~~~~~~~~~~

.. code:: text

    AddInvoice [App -> Node]
    ===========
    - invoiceId
    - totalDestPayment


    CancelInvoice [App -> Node]
    ==============
    - invoiceId


    CommitInvoice [App -> Node]
    ==============
    - MultiCommit

Suppose that the seller has a website, and a user wants to buy an item
on the website. The seller will perform the following:

-  Generate a new ``invoiceId`` and send ``AddInvoice`` to the Seller's
   Node.
-  Send an invoice to the buyer (Out of band), and wait for Commit from
   the buyer.
-  If waited too long for confirmation, send ``CancelInvoice`` to node
   and forget about this transaction.
-  Receive a confirmation from the buyer (Out of band).
-  Send ``MultiCommit`` to the Node.
