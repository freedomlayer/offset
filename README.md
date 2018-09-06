# CSwitch

[![Build Status](https://travis-ci.com/realcr/cswitch.svg?token=BTq7pyQeAJ7BsmCssexj&branch=master)](https://travis-ci.com/realcr/cswitch)
[![codecov](https://codecov.io/gh/kamyuentse/cswitch/branch/master/graph/badge.svg?token=8wnbKAjDFl)](https://codecov.io/gh/kamyuentse/cswitch)
[![Gitter chat](https://badges.gitter.im/freedomlayer/cswitch.svg)](https://gitter.im/freedomlayer/cswitch)

**Credit Switching** is a decentralized payment infrastructure, relying on real
world trust between people. CSwitch is a Credit Switching engine written in Rust.

CSwitch is still a work in progress, and is not yet ready for use in production.

## Setting up development environment

Theoretically CSwitch should work anywhere Rust works (Windows, Linux, MacOS).

### Dependencies

- [SQLite3][sqlite], for persistent storage.
- [Cap'n Proto][capnp], for serialization and deserialization.

[sqlite]: https://www.sqlite.org
[capnp]: https://capnproto.org

Also, we need the Rust development toolchain.

### Install dependencies and toolchain

- Install Rust development toolchain, we recommend [rustup](https://rustup.rs).
- Install SQLite3:
    - On Ubuntu, run: `sudo apt install libsqlite3-dev`
    - On MacOS SQLite3 is part of the system
- Install capnproto:
    - On Ubuntu, run: `sudo apt install capnproto`
    - On MacOS, run: `brew install canpnp`

### Pinned toolchain version

We currently pin the version of `Rust` and `clippy`, the current version in
use can be found in `.travis.yml`. We do this because things tend to break very
often when using the latest nightly version.

To use a pinned rust toolchain with this project, run:

```bash
rustup override set nightly-YYYY-MM-DD
```

Where the current `YYYY-MM-DD` can be found by looking at `.travis.yml`.

To install [clippy](https://github.com/rust-lang-nursery/rust-clippy) which
matches the installed Rust toolchain, run:

```bash
rustup update
rustup component add clippy-preview
```
