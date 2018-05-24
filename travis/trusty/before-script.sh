#!/usr/bin/env bash

set -eux -o pipefail

CLIPPY_VERSION="0.0.204"
CLIPPY_COMMIT_HASH="e7a3e03c6e"

if [[ -f ".cargo/config" ]]; then
    rm .cargo/config
elif [[ ! -d ".cargo" ]]; then
    mkdir .cargo
fi

echo "[target.$TARGET]" > .cargo/config
echo "linker= \"$CC\"" >> .cargo/config

if [[ -f "$HOME/.cargo/bin/cargo-clippy" ]]; then
    if [[ "$(cargo clippy --version)" -ne "$CLIPPY_VERSION" ]]; then
        cargo install --force clippy --rev $CLIPPY_COMMIT_HASH
    fi
else
    cargo install clippy --rev $CLIPPY_COMMIT_HASH
fi

cat .cargo/config

travis/trusty/install-capnp.sh
