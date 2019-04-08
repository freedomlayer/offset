#!/usr/bin/env bash

# Install cargo-sweep. If installed, cargo failure will be ignored.
cargo install cargo-sweep --vers 0.4.1 || true

# cargo-sweep produces a lot of output
cargo sweep . -t 30 > /dev/null
