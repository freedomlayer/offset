Economic idea
=============

In traditional monetary systems, money is issued by the state. (For example, the
US dollar is issued by the USA government). Participants of the market can then
use the money issued by the state to buy and sell goods and services in the
market.

**Offset moves the power of issuing money from the state to the market
participants.** 


Trading without money
---------------------

To understand how Offset works, we begin with an example. Consider two local
merchants, Charli and Bob, who work at the same street. Charli makes chocolate
bars and sells them. Bob bakes bread. Living in the same community, Charli
often buys bread from Bob's store, and Bob often buys chocolate from Charli's
store.

TODO: Image here showing Bob and Charli.

To clarify our understanding about the trading process, let's
*imagine that Bob and Charli have no money at all*. Now, given that Charli has
something that Bob wants and vice versa, how can they perform the exchange
without money?

Traditional Loans
~~~~~~~~~~~~~~~~~

**In the traditional money system, all money is created through borrowing from banks.**
Hence, a possible solution for Bob's and Charli's trade problem is to take a
loan. Charli could go to a bank, ask for a loan [1]_, and then use that money
to buy some bread from Bob. At his moment, new money has entered the system.
Bob can later use that money to buy chocolate bars from Charli. If Charli could
sell enough chocolate bars, she will be able eventually to repay the loan to
the bank [2]_.

TODO: Image showing loan process and repayment.

The loan solution has some major disadvantages for Charli and Bob: When Charli
takes the loan, she promised to the bank to return more money at a later time,
usually in the form of `interest rate`_. As a result, Charli will have to
increase her chocolate bars prices to be able to return the loan she took from
the bank. Bob will also have to increase his bread prices if he wants to keep
buying the same amount of chocolate bars from Charli. 

In other words, the exchange between Bob and Charli becomes less efficient, as
some of the value is leaking from the mutual relationship to an external
entity: the bank.


Barter
~~~~~~

What if Bob and Charli lived in a place without money? Could they still manage
to exchange without an external entity that creates money?

Charli could ask Bob to directly exchange one bar of chocolate from her store
for two loafs of bread from Bob's bakery [3]_. If Bob agrees, the a deal can be
made. This sort of direct exchange is called barter_.

TODO: Image here showing barter

Barter allows to trade in a moneyless world, but by itself it is not always a
convenient solution. 

First, the value of goods do not always behave nicely as in the example
above. What if one bar of Charli's chocolate has the same value of one and a
half Bob's loafs of bread? To resolve this issue, Charli might trade two
chocolate bars for three loafs of bread, but this is inefficient.

Second, barter trade requires that each side wants something that the other
side wants, at about the same time. What if Charli wants to buy bread today,
but Bob doesn't want to buy chocolate bars today? Exchange will not be
possible.

Mutual credit
~~~~~~~~~~~~~

To release themselves from the strictness of the barter system, Bob and Charli
can come up with the following system: they maintain a single paper that
contains the current mutual balance between Bob and Charli. 

At first, the balance will be zero:

TODO: Image: Bob owes Charli: 0

Now when Bob wants to buy a chocolate bar from Charli's store, Charli hands Bob
a chocolate bar, and they both agree to move the balance to 2USD [4]_. **At
that moment, new money was created.** What this balance means is that Bob now
owes Charli 2USD, and Charli can use those 2USD anytime she wants to buy bread
at Bob's bakery. We call this idea `Mutual credit`_.

Mutual credit allows Bob and Charli to split the transaction into two
separate parts that could happen in separate place and time, compared to
barter which requires everything to happen at once.

There are a few points to consider here. First, the creation of money:
Initially Bob and Charli both had no money, but Bob was able to create new
money during the purchase of the chocolate bar from Charli. Bob didn't need any
help from external entities to create the money.

If in the future Charli ever arrives at Bob store and buys two loafs of bread for
exactly 2USD, the balance between Bob and Charli will again be restored to 0.
**At that moment, money was destroyed**


Credit limits
-------------

One thing we haven't talked about yet, is what could go wrong.  

Consider the mutual credit line between Bob (the baker) and Charli (the
chocolate artist). How can Bob be sure that Charli doesn't one day buy lots of
bread from his bakery, runs away with the bread and never comes back? Bob will
be left with a piece of paper saying that Charli owes him money, but this piece
of paper does not grant Bob any buying power.

To handle this issue, we introduce credit limits. For example, Bob will declare
ahead of time that Charli can owe him a maximum of 200USD, and Charli will
declare that Bob can owe her a maximum of 150USD [5]_. Therefore, if Charli will
keep on buying bread without selling chocolate, eventually Bob will not be
willing to provide any more bread.

Charli might still decide to take as much bread as it can and disappear one
day, but with credit limits, Bob knows that the maximum amount he may lose from
the relationship with Charli is 200USD. 

By disappearing and not respecting her credit line with Bob, Charli has lost
the mutual relationship with Bob. The overall cost of the lost relationship
probably costs more than the money gained by Charli.


Offset: Mutual credit at scale
------------------------------

Earlier we described how two merchants can manage a mutual credit line. Using a
piece of paper to write the mutual balance can be a reasonable solution for a
market of two people, but it could get difficult quickly in a market with
multiple participants. **Offset was designed to allow using mutual credit
at scale.**

Continuing our story, consider an extra market participant, called Daniel.
Daniel is a mechanic, repairing cars for living. Daniel lives in the same
community as Bob (the baker), and Charli (the chocolate artist). Daniel wants
to be able to buy from Bob and Charli, and also provide his car repair services.

Bob and Charli manage a mutual credit line between each other. Let us assume
that Daniel and Charli are good friends, and they also set up a mutual credit
line. This is the resulting graph of relationships:


.. code:: text

      TODO: Image

           0         0
      B ---|--- C ---|--- D


In the figure above, we denote B: Bob, C: Charli and D: Daniel.
We assume that initially the balance between Bob and Charli is 0, and that the
balance between Charli and Daniel is also 0. We also assume that both Bob and
Charli, and Charli and Daniel has set up some credit limits.

In Offset we denote the relationship between Bob and Charli, or between Charli
and Daniel, as **friendship**.

We have already seen how Bob and Charli can trade, and in the same way Charli and
Daniel can trade. What is new about this configuration is the discovery that
Bob and Daniel can also trade, although they do not have a direct mutual credit
line between each other.

Assume that Bob arrives at Daniel's garage to repair his car, and the repair
cost 100USD. Bob can push the credits all the way to Daniel through Charli:
1. Bob owes 100USD to Charli
2. Charli owes 100USD to Daniel

resulting state will look like this:

.. code:: text

      TODO: Image showing the purchase process

        -100      -100
      B --|---- C --|---- D

Note that the total balance of Charli (-100USD + 100USD = 0USD) hasn't changed as a result of
the transaction between Bob and Daniel. Bob's total balance decreased by
100USD, and Daniel's total balance increased by 100USD.

As a market gets larger, routes of mutual credit lines between people might get
longer and more dynamic, hence more difficult to discover. In addition, it
might become more difficult to ensure a transaction performed along a long
route is not stalled, or fails due to lack of synchronization. Offset is a
technology that solves those issues, allowing automatic discovery of routes and
synchronization guarantees for payments.


Fees
----

Earlier we described how Bob can buy from Daniel, through a route along Charli.

.. code:: text

      TODO: Image

           0         0
      B ---|--- C ---|--- D


With Offset, Charli's phone (or computer) will mediate the transaction
automatically, without any human intervention. In some cases Charli might
decide to collect fees for mediating the transaction. This could be to mitigate
risk, or due to expenses of running an Offset instance [6]_.

Offset allows setting up fee in the form of ``a% + b``, where ``a`` is the amount
of percents taken from the transaction, and ``b`` is a constant amount. For
example, ``0.5% + 0.01`` that for a ``100USD`` transaction sent from Bob to Daniel,
Bob will have to pay Charli an extra of ``0.5 + 0.01 = 0.51USD``.

Offset's algorithm for discovering routes for payment generally prefers routes
with lower fees over routes with higher fees. This allows open competition for
fees.


.. [1] 
   Considering a closed system including only Charli and Bob, If Charli was
   able to repay the loan (with interest) then it definitely means Bob has less
   than 0 "money", which means he is bankrupt! How can this be? In our modern
   economy, more and more money is created all the time. This is a strategy
   called inflation.
.. [2] 
   Or borrowing credit from a credit card company
.. [3] 
   Charli happens to own one of the only chocolate stores in town, and
   therefore she can price her chocolate bars higher than what Bob can price
   his loafs of bread.
.. [4] 
   In fact, Bob and Charli could decide upon any currency that fits them, or
   even invent a new currency. USD was chosen here because of the assumption
   most readers are familiar with it.
.. [5] 
   The credit limits don't have to be equal! In some cases it might be possible
   that one party trusts the other party more than the other way around. In
   might also be true that certain businesses might have different turnover,
   and therefore might need different amount of credit to operate.
.. [6] 
   For example, if Charli is an organization running an Offset cloud
   instance.

.. _`interest rate`: https://en.wikipedia.org/wiki/Interest_rate
.. _barter: https://en.wikipedia.org/wiki/Barter
.. _`Mutual credit`: https://en.wikipedia.org/wiki/Mutual_credit
