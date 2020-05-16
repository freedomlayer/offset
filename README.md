# Offset

[![Build Status](https://travis-ci.com/freedomlayer/offset.svg?branch=master)](https://travis-ci.com/freedomlayer/offset)
[![codecov](https://codecov.io/gh/freedomlayer/offset/branch/master/graph/badge.svg)](https://codecov.io/gh/freedomlayer/offset)

**Offset** is a decentralized payment system, allowing to pay and process
payments efficiently and safely.

**Offset** is a credit card powered by trust between people. 

Warning: Offset is still a work in progress, and is not yet ready for use in
production.

## Info

- [Documentation](https://docs.offsetcredit.org)
- [Blog](https://www.freedomlayer.org/offset/)

## License

- The core crates of Offset are licensed under the AGPL-3.0 license.
- The crates used as interface for building Offset apps are licensed under the MIT
    or Apache 2.0, at your option.

Each crate license info can be found in the corresponding crate directory and in
the crate's `Cargo.toml`.


## Download

[Releases page](https://github.com/freedomlayer/offset/releases)

## Dockerized Offset servers

[offset_docker](https://github.com/freedomlayer/offset_docker)


## Building Offset

### Install dependencies

- Install [Rust](https://www.rust-lang.org/tools/install).
- Install [capnproto](https://capnproto.org):
  - On Ubuntu, run: `sudo apt install capnproto`
  - On MacOS, run: `brew install canpnp` 

### Rust toolchain version

Offset builds on stable! 
The toolchain is pinned using the `rust-toolchain` file.

For testing, run:

```
cargo test
```

To build, run:

```
cargo build --release
```


### Development tools

If you want to hack on Offset, run the following commands to install clippy,
rustfmt and rls:

```bash
rustup update
rustup component add clippy
rustup component add rustfmt
rustup component add rls rust-analysis rust-src
```
