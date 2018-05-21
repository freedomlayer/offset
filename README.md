# CSwitch

[![Build Status](https://travis-ci.com/realcr/cswitch.svg?token=BTq7pyQeAJ7BsmCssexj&branch=master)](https://travis-ci.com/realcr/cswitch)
[![codecov](https://codecov.io/gh/kamyuentse/cswitch/branch/master/graph/badge.svg?token=8wnbKAjDFl)](https://codecov.io/gh/kamyuentse/cswitch)
[![Gitter chat](https://badges.gitter.im/freedomlayer/cswitch.svg)](https://gitter.im/freedomlayer/cswitch)

A Credit Switching engine written in Rust.

## Setting up development environment

Theoretically CSwitch should work anywhere Rust works (Windows, Linux, MacOS).

- [Install Rust](https://www.rust-lang.org/install.html). We currently use
    nightly Rust. You should pin to exact nightly Rust version, this could be
    done using the command: `rustup override set nightly-YYYY-MM-DD`. Note that
    this command should be run at the root of the repository. The current
    nightly version could be found in .travis.yml.

- Install libsqlite3-dev. On ubuntu, run `sudo apt install libsqlite3-dev`.
- [Install capnproto](https://capnproto.org/install.html). On Ubuntu, run `sudo apt install capnproto`

After all is done, run 

```bash
cargo test
```

to make sure that all tests pass.
