The Economic idea
==================

In traditional monetary systems, money is issued by the state. (For example, the
US dollar is issued by the USA government). Participants of the market can then
use the money issued by the state to buy and sell goods and services in the
market.

**Offset moves the power of issuing money from the state to the market
participants.** 

That is the single most important property of Offset.

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

Loans
~~~~~

**In the traditional money system, all money is created through borrowing from banks.**
Hence, a possible solution for Bob's and Charli's trade problem is to take a
loan. Charli could go to a bank, ask for a loan [4]_, and then use that money
to buy some bread from Bob. At his moment, new money has entered the system.
Bob can later use that money to buy chocolate bars from Charli. If Charli could
sell enough chocolate bars, she will be able eventually to repay the loan to
the bank [5]_.

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
for two loafs of bread from Bob's bakery [6]_. If Bob agrees, the a deal can be
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
a chocolate bar, and they both agree to move the balance to 2USD [7]_. **At
that moment, new money was created.** What this balance means is that Bob now
owes Charli 2USD, and Charli can use those 2USD anytime she wants to buy bread
at Bob's bakery. We call this idea `Mutual Credit`_, and this is the underlying
system powering Offset.

Mutual credit allows Bob and Charli to split the transaction into two
separate parts that could happen in separate place and time, compared to
barter which requires everything to happen at once.

There are a few points to consider here. First, the creation of money:
Initially Bob and Charli both had no money, but Bob was able to create new
money during the purchase of the chocolate bar from Charli. Bob didn't need any
help from external entities to create the money.

If in the future Charli ever arrives at bob store and buys two loafs of bread for
exactly 2USD, the balance between bob and charli will again be restored to 0.
**At that moment, money was destroyed**


Offset payments
---------------



.. [4] 
   Considering a closed system including only Charli and Bob, If Charli was
   able to repay the loan (with interest) then it definitely means Bob has less
   than 0 "money", which means he is bankrupt! How can this be? In our modern
   economy, more and more money is created all the time. This is a strategy
   called inflation.
.. [5] 
   Or borrowing credit from a credit card company
.. [6] 
   Charli happens to own one of the only chocolate stores in town, and
   therefore she can price her chocolate bars higher than what Bob can price
   his loafs of bread.
.. [7] 
   In fact, Bob and Charli could decide upon any currency that fits them, or
   even invent a new currency. USD was chosen here because of the assumption
   most readers are familiar with it.
      

.. _`interest rate`: https://en.wikipedia.org/wiki/Interest_rate
.. _barter: https://en.wikipedia.org/wiki/Barter
.. _`Mutual Credit`: https://en.wikipedia.org/wiki/Mutual_credit
