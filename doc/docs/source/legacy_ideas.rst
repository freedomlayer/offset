Legacy Offset research
======================

Proportional credit freezing allocation (Legacy)
------------------------------------------------

To solve the credit freezing DoS problem, we limit the amount of credit
that can be frozen. This is done as follows:

Consider a node A, having friendship relationships with B, C, D, E.

.. code:: text

     B <---  /---\
     C <---  | A |  <--- E
     D <---  \---/

When a RequestSendFund is sent from E through A to one of B, C, D,
credits are frozen from E to A.

We distinguish between different frozen credits from E to A:

-  ``F{EA}_B``: Frozen credits that represent a RequestSendFund that
   continues to B.
-  ``F{EA}_C``: Frozen credits that represent a RequestSendFund that
   continues to C.
-  ``F{EA}_D``: Frozen credits that represent a RequestSendFund that
   continues to D.

Recall that ``md{AI}`` is the maximum debt A allows the node I. This is
an amount controlled by A. We denote by ``md{A}``: the sum of maximum
debt A allows all the nodes A has friendship with. In our case:
``md{A} = md{AB} + md{AC} + md{AD} + md{AE}``.

We then require:

``F{EA}_I / md{AE} <= md{AI} / (md{A} - md{AE})``

In words: The amount of frozen credits from the channel ``E-->A``
allocated to the channel ``A-->I`` is proportional to the trust A puts
in I (For all nodes I that have friendship with node A).

More generally (Instead of only looking at ``A <-- E``), we require
that:

``F{JA}_I / md{AJ} <= md{AI} / (md{A} - md{AJ})``

For all nodes I, J that have friendship with A. (Checking for the
special case of I = J, we get ``F{IA}_I = 0``. Putting this into the
inequality yields no new information).

In words: The amount of frozen credits from the channel ``J-->A``
allocated to the channel ``A-->I`` is proportional to the trust A puts
in I.

This requirement can also be formulated as:

``F{JA}_I <= md{AI} * md{AJ} / (md{A} - md{AJ})``

Note that the right hand of this inequality is fully determined by A's
configuration.

Exponential decay of credit freezing (Legacy)
---------------------------------------------

Proportional credit freezing allocation makes sure that all the
immediate friends of A can freeze credits only according to the amount
of trust A puts in them. This is a "first level" protection against
freezing DoS.

However, note that this countermeasure is not enough to protect against
excessive freezing of credits by remote adversarial nodes. Consider for
example the following friends topology:

.. code:: text

             F                  -- J
             |                     |
             |                     |
        C -- A -- B
             |
             |
             E

Assume that the node J is an adversarial node. J can start a Request
messages with the following paths (All start with J and end with J):

-  (J, ..., E, A, B, ... J)
-  (J, ..., E, A, C, ... J)
-  (J, ..., E, A, F, ... J)

Assume that J does not send Response messages for the initiated Request
messages. This will allow J to consume almost all of the available
credits from E to A as frozen credits.

If we could somehow extend the proportional credit freezing allocation
to the whole friends graph we could have gained better protection
against freezing DoS for the node A, as every node will be able to
freeze credits exactly according to the derived trust the node A puts in
him. (By derived trust we mean multiplying the relative trust a node
puts in another along the chain from one node to another)

This is probably not possible though, as the node A can not have a
reliable view of the network of friends. In addition, the network of
friends changes with time, while frozen credits can not be changed after
freezing takes place.

To limit the amount of frozen credits a remote node can create, we use
**exponential decay** of allowed credit freezing. Consider a node A, and
a path going through A as follows:

.. code:: text

    A1 -- A2 -- ... -- A(k)

The maximum amount of credits that can be frozen (from the point of view
of the node A(i)) in this path are:

.. code:: text

    frozenCredits[A(i)->A(k)] <= md{A(i)-A(i-1)} * (md{A(i)-A(i+1)} / (md{A(i)} - md{A(i)-A(i-1)}))
        * (md{A(i+1)-A(i+2)} / (md{A(i+1)} - md{A(i+1)-A(i)}))
        ...
        * (md{A(k-1)-A(k)} / (md{A(k-1)} - md{(A(k-1)-A(k-2)}))

Where ``frozenCredits[A(i)->A(k)]`` is the total amount of credits
frozen from ``A(i)`` to ``A(k)`` in all current open requests, including
the newly proposed request.

Every node A(i) in the chain must check this inequality, and return
error if the inequality is not satisfied. Therefore, every node A(s)
that forwards a request message will add the following information to
the forwarded Request message:

-  sharedCredits(s): md{A(s)-A(s-1)} if A is not first, md{A(s)}
   otherwise.

-  forwardTrust(s): md{A(s)-A(s+1)}

-  totalTrust(s): md{A(s)} - md{A(s)-A(s-1)} if A is not first, md{A(s)}
   otherwise

Upon receipt of a Request message, a node A(m-1) will verify the
following frozen credits inequalities before forwarding the request to
A(m):

.. code:: text

    frozenCredits[1->m] <= sharedCredits1 * (forwardTrust1 / totalTrust1) * ...
                        * (forwardTrust(m-1) / totalTrust(m-1))

    frozenCredits[2->m] <= sharedCredits2 * (forwardTrust2 / totalTrust2) * ...
                        * (forwardTrust(m-1) / totalTrust(m-1))

    ...

    frozenCredits[(m-1)->m] <= sharedCredits(m-1) * (forwardTrust(m-1) / totalTrust(m-1))

Where ``frozenCredits[i->m]`` is the total amount of credits frozen from
``A(i)`` to ``A(m)`` in all current open requests, including the newly
proposed request.

If any of the above inequalities is not satisfied, the node A(m-1) will
return a failure message. Note that we can only trust non adversarial
nodes to verify the above inequalities.

As a result of the proportional credit freezing allocation mechanism, we
obtain this property: The farer a node from a channel in the friendship
graph (with respect to trust), the less credits he can freeze. The
amount of credits that can be frozen decrease exponentially with the
distance.

This means that each single RequestSendFund message will allow to freeze
(and therefore send) less credits. However, large amounts of credits can
still be sent to a remote destination by issuing multiple
RequestSendFund messages.

With the proportional credit freezing mechanism, an attacker can only
freeze credits according to the amount of trust put on him, possibly
indirectly. The farer an attacker in the friendship graph from a friends
channel, the less credits the attacker can freeze.

TODO: Possibly add examples here?

Note that ``frozenCredits[i->m]`` above is the total amount of credits
frozen from the node A(i) to the node A(m). This value is calculated by
A(m-1) by adding the frozen credits from all the current open requests
from the node A(i) to the node A(m), including the newly proposed
request.

The idea of time limiting requests (Abandoned)
----------------------------------------------

In the past we considered the idea of adding a time limit for a request
(In the layer of communication or funds transfer), but we abandoned it
due to problems of inconsistencies that could arise in the mutual credit
management (between neighbors or friends).

We show an example of implementing the time limit feature for a request.
Consider the following graph of friends:

.. code:: text

    A -- B -- C -- D

A wants to transfer funds to D with a time limit for the request. A is
willing to wait at most 6 seconds before the request is fulfilled. A
sends B a request message that contains:

-  A promise for 6 credits upon presenting a signed response message
   from D.
-  Time remaining fulfill the request: 6 seconds.

B sends C a request message that contains:

-  A promise for 4 credits upon presenting a signed response message
   from D.
-  Time remaining to fulfill the request: 4 seconds.

C sends D a request message that contains:

-  A promise for 2 credits upon presenting a signed response message.
-  Time remaining to fulfill the request: 2 seconds.

D then sends a signed response message back to C. If the signed response
message was sent on time (Before 2 seconds passed), C will accept the
message and pay D 2 credits. C then sends back the signed response
message to B. If sent on time, B will accept the message and pay C 4
credits. Finally B sends back the signed response message to A. If the
response message arrived on time, A will pay B the promised 6 credits.

The idea time limiting requests was abandoned because of the way it
performs when network failures happen. Consider the previous example of
A attempting to send funds to D, when A is willing to wait at most 6
seconds.

Assume that A sends a request message to B, B sends a request message to
C, C sends a request message to D. Next, D sends C a signed response
message. C pays D the promised 2 credits, and then C attempts to send
back to B the signed response message, but suddenly the connection
between B and C is disturbed.

.. code:: text

    A -- B -X- C -- D

If the connection is disturbed for a few seconds, B will send back to A
an error message indicating that the request could not be fulfilled. B
will be paid 1 credit by A.

C is at a loss of 2 credits. C thinks that he managed to send the signed
response message to B on time. However, B has never received the signed
response message. D has received the funds, but A is not sure that the
transaction is completed.

There is conflict in the mutual credit state between B and C. C thinks
that B should pay him 4 credits, while B thinks that C didn't send the
signed response message on time.

Those kind of conflicts are difficult to solve, because it is not known
who is at blame in this situation. It is possible that the communication
between B and C was disturbed due to some problem that is outside of the
control of both B and C.

Because of the possibility of conflicts that can not be resolved easily,
**we decided to abandon the idea of time limiting requests.**
