#!/usr/bin/env bash

CARGO_SWEEP_VERSION=0.4.1

# Install cargo-sweep if absent
if [ ! -f ~/.cargo/bin/cargo-sweep ]; then
    cargo install cargo-sweep --vers $CARGO_SWEEP_VERSION
fi

FOUND_VERSION=$(grep cargo-sweep ~/.cargo/.crates.toml | cut -d' ' -f2)

# Update cargo-sweep if necessary
if [ "$FOUND_VERSION" != "$CARGO_SWEEP_VERSION" ]; then
    cargo install cargo-sweep --vers $CARGO_SWEEP_VERSION --force
fi

echo Rust toolchain version: $TRAVIS_RUST_VERSION

# cargo-sweep produces a lot of output
cargo sweep --toolchains $TRAVIS_RUST_VERSION > /dev/null
