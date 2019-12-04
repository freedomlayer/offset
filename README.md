# Offst

[![Build Status](https://travis-ci.com/freedomlayer/offst.svg?branch=master)](https://travis-ci.com/freedomlayer/offst)
[![codecov](https://codecov.io/gh/freedomlayer/offst/branch/master/graph/badge.svg)](https://codecov.io/gh/freedomlayer/offst)
[![Documentation Status](https://readthedocs.org/projects/offst/badge/?version=latest)](https://offst.readthedocs.io/en/latest/?badge=latest)
[![Gitter chat](https://badges.gitter.im/freedomlayer/offst.svg)](https://gitter.im/freedomlayer/offst)

**Offst** is a decentralized payment system, allowing to pay and process
payments efficiently and safely.

Warning: Offst is still a work in progress, and is not yet ready for use in production.

## Info

- [Documentation](https://offst.readthedocs.io/en/latest/?badge=latest)
- [Blog](https://www.freedomlayer.org/offst/)

## License

- The core crates of Offst are licensed under the AGPL-3.0 license.
- The crates used as interface for building Offst apps are licensed under the MIT
    or Apache 2.0, at your option.

Each crate license info can be found in the corresponding crate directory and in
the crate's `Cargo.toml`.


## Download

[Releases page](https://github.com/freedomlayer/offst/releases)

## Dockerized Offst servers

[offst_docker](https://github.com/freedomlayer/offst_docker)


## Building Offst

### Install dependencies

- Install [Rust](https://www.rust-lang.org/tools/install).
- Install [capnproto](https://capnproto.org):
  - On Ubuntu, run: `sudo apt install capnproto`
  - On MacOS, run: `brew install canpnp` 

### Rust toolchain version

Offst builds on stable! 
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

If you want to hack on Offst, run the following commands to install clippy,
rustfmt and rls:

```bash
rustup update
rustup component add clippy
rustup component add rustfmt
rustup component add rls rust-analysis rust-src
```
