# Contributing guide

## Offset Code guidelines

This document contains the main values and priorities when writing Offset
code. If you are unsure about a design or technical decision, this document
might be able to help you reach a decision.

### Priorities

The main priorities to consider when writing code in Offset are as follows (In
this order):

1. Security and safety
2. Correctness
3. Easy to read code
4. Efficiency

### Security and Safety

Offset is used for passing funds between people. Therefore safety is of
highest priority.

- When two possible designs are possible, always choose the safer design.
- Always prefer using a boring well tested cryptographic library over writing
    your own.
- Random for cryptographic uses should only be obtained from Cryptographic sources.
- Do not use `unsafe` statements.
- In most cases, prefer to use a compile time safety guarantee over a runtime safety guarantee.
    Instead of verifying that a state is valid at runtime, design your data
    structures such that invalid states are unrepresentable.
- Safe operations with numbers. There are Rust functions for safe operations
    with numbers (addition, subtraction, multiplication, safe casting etc.).
    Always prefer those over checking inequalities manually.

### Async tips

- Async code is hard to read and hard to test, therefore it should be used only
    when necessary.
- If you have to use async code, try to separate all the synchronous logic into
    a separate state machine that is easy to test and easy to reason about.
- "Send the sender" trick: If component C sends a request to component D and
    waits for a response, C should first create a `mpsc::oneshot()` and keep the
    receiving end. Then C sends the message to D, together with the sending end
    of the oneshot. Upon receipt, D will process the message, and send the
    response through the sending end of the oneshot.

### Tests

When writing your code, always think about how you are going to test it.
If you think that testing your code will be difficult, try to come up with a
different design that will make testing easier.

Always make sure that your tests are deterministic. If a test is not
deterministic, it can sometimes fail and sometimes succeed. A failure for a non
deterministic test can be hard to track.

Do not use real random number generators in your tests. Instead, load a random
number generator with a deterministic seed.

Do not rely on time in your tests. Time is not deterministic. Whenever time is
required in your code, use a Stream of ticks instead.

An exception for the deterministic tests rule can be made for large integration tests.
