#!/usr/bin/env bash

# Install cargo-sweep. If installed, cargo failure will be ignored.
# TODO: parse ~/.cargo/.crates.toml to determine version
#       maybe use cargo-update library for that
cargo install cargo-sweep --vers 0.4.1 || true

echo Rust toolchain version: $TRAVIS_RUST_VERSION

# cargo-sweep produces a lot of output
cargo sweep --toolchains $TRAVIS_RUST_VERSION > /dev/null
