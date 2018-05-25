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
    - For Ubuntu user, run `sudo apt install libsqlite3-dev`
    - For MacOS user, the SQLite3 is part of the system
- Install capnproto:
    - For Ubuntu user, run `sudo apt install capnproto`
    - For MacOS user, it can be installed via Homebrew: `brew install canpnp`

### Note for the toolchain version

Currently we pinned the version of `Rust` and `clippy`, the newest version we used
can be found in `.travis.yml`.

You can run `rustup override set nightly-YYYY-MM-DD` to pinned Rust toolchain version
under the root of the project, and `cargo install clippy --rev <commit hash>` to do
the same for `clippy`.

## Code of conduct

Before you open a PR, all test should passed, also check the output of `clippy`,
promise error free and make the warning as little as possible.
