#!/usr/bin/env bash

set -eux -o pipefail

CLIPPY_VERSION="0.0.212"

if [[ -f ".cargo/config" ]]; then
    rm .cargo/config
elif [[ ! -d ".cargo" ]]; then
    mkdir .cargo
fi

# Clean any cache from cargo registry:
rm -rf $HOME/.cargo/registry

echo "[target.$TARGET]" > .cargo/config
echo "linker= \"$CC\"" >> .cargo/config

if [[ -f "$HOME/.cargo/bin/cargo-clippy" ]]; then
    if [[ "$(cargo clippy --version)" != "$CLIPPY_VERSION" ]]; then
        cargo install --force clippy --vers "=$CLIPPY_VERSION"
    fi
else
    cargo install clippy --vers "=$CLIPPY_VERSION"
fi

cat .cargo/config

travis/trusty/install-capnp.sh
