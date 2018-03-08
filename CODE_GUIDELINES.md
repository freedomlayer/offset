
# Guidelines for CSwitch code

This document contains the main values and priorities when writing CSwitch
code. If you are unsure about a design or technical decision, this document
might be able to help you reach a decision.


## Priorities

The main priorities to consider when writing code in CSwitch are as follows (In
this order):

1. Security and safety
2. Correctness
3. Easy to read code
4. Quick implementation
5. Efficiency


## Guidelines

### Security and Safety

CSwitch could be used as a distributed bank. Besides the ability to transfer
funds, the most important feature of a bank is safety. Therefore safety is of
highest priority.

- When two possible designs are possible, always choose the safer design.
- Always prefer using a boring well tested cryptographic library over writing
    your own.
- Random for cryptographic uses should only be obtained from Cryptographic sources.
- Do not use `unsafe` statements, only if really necessary.
- In most cases, prefer to use a compile time guarantee over a runtime guarantee.
    Instead of verifying that a state is valid at runtime, design your data
    structures such that invalid states are unrepresentable.
- Safe operations with numbers. There are Rust functions for safe operations
    with numbers (addition, subtraction, multiplication, safe casting etc.).
    Always prefer those over checking inequalities manually.


### Easy to read code

They say that in order to debug a piece of code, one has to become smarter than
the one who wrote it. As a conclusion, if you write the smartest code you can,
you will not be able to debug it. Prefer to write simple code that is easy to
reason about. 


### Async tips

- Async code is hard to read and hard to test, therefore it should be used only
    when necessary.
- If you have to use async code, try to separate all the synchronous logic into
    a separate state machine that is easy to test and easy to reason about.
- "Send the sender" trick: If component C sends a request to component D and
    waits for a response, C should first create a mpsc::oneshot() and keep the
    receiving end. Then C sends the message to D, together with the sending end
    of the oneshot. Upon receipt, D will process the message, and send the
    response through the sending end of the oneshot.
- Prefer to use moves instead of borrows when using Futures. Avoid borrows and
    lifetimes when using Futures with combinators.

### Tests

When writing your code, always think about how you are going to test it.
If you think that testing your code will be difficult, you should probably
change your design.


### Rust

- In most cases, prefer the design with less lifetime hints. Every time you add
    a new lifetime hint, an angel dies.
