# CSwitch

[![Build Status](https://travis-ci.com/realcr/cswitch.svg?token=BTq7pyQeAJ7BsmCssexj&branch=master)](https://travis-ci.com/realcr/cswitch)
[![codecov](https://codecov.io/gh/kamyuentse/cswitch/branch/master/graph/badge.svg?token=8wnbKAjDFl)](https://codecov.io/gh/kamyuentse/cswitch)
[![Gitter chat](https://badges.gitter.im/freedomlayer/cswitch.svg)](https://gitter.im/freedomlayer/cswitch)

A Credit Switching engine written in Rust.

## Setting up development environment

Theoretically CSwitch should work anywhere Rust works (Windows, Linux, MacOS).

### Requirement

- [SQLite3][sqlite], for persistent storage.
- [Cap'n Proto][capnp], as ser&de protocol.

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

### Note for the toolchain version

We currently pin the version of `Rust` and `clippy`, the current version in
use can be found in `.travis.yml`. We do this because things tend to break very
often when using the latest nightly version.

You can run `rustup override set nightly-YYYY-MM-DD` to pinned Rust toolchain version
under the root of the project. 

To install [clippy](https://github.com/rust-lang-nursery/rust-clippy) which
matches the installed Rust toolchain, run:

```bash
rustup update
rustup component add clippy-preview
```
