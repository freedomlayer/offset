#!/usr/bin/env bash

# Install cargo-sweep. If installed, cargo failure will be ignored.
# TODO: parse ~/.cargo/.crates.toml to determine version
#       maybe use cargo-update library for that
cargo install cargo-sweep --vers 0.4.1 || true

RUST_VERSION=$(head -n 1 rust-toolchain)
echo "Rust toolchain version: $RUST_VERSION"

# cargo-sweep produces a lot of output
cargo sweep --toolchains "$RUST_VERSION" > /dev/null
