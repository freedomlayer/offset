# Offst

[![Build Status](https://travis-ci.com/freedomlayer/offst.svg?branch=master)](https://travis-ci.com/freedomlayer/offst)
[![codecov](https://codecov.io/gh/freedomlayer/offst/branch/master/graph/badge.svg)](https://codecov.io/gh/freedomlayer/offst)
[![Gitter chat](https://badges.gitter.im/freedomlayer/offst.svg)](https://gitter.im/freedomlayer/offst)

**Offst** is a decentralized payment infrastructure, relying on mutual trust. See [Offst's
blog](https://www.freedomlayer.org/offst/) for more information.

Warning: Offst is still a work in progress, and is not yet ready for use in production.

## Building Offst

### Install dependencies:

- Install [Rust](https://www.rust-lang.org/tools/install).
- Install [capnproto](https://capnproto.org):
  - On Ubuntu, run: `sudo apt install capnproto`
  - On MacOS, run: `brew install canpnp` 

### Pinned toolchain version

Offst currently only compiles on Rust nightly (Required for async support).
Things change quickly on nightly, therefore to avoid breakage we pin the version of the rust
compiler to a specific version, and we bump it once in a while. The pinned
version can be found in `.travis.yml`.

To use a pinned rust toolchain with this project, run:

```bash
rustup override set nightly-YYYY-MM-DD
```

Where the current `YYYY-MM-DD` can be found by looking at `.travis.yml`.

### Development tools

If you want to hack on Offst, run the following commands to install clippy,
rustfmt and rls:

```bash
rustup update
rustup component add clippy
rustup component add rustfmt
rustup component add rls rust-analysis rust-src
```
