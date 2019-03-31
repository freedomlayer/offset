#!/usr/bin/env bash

set -eux -o pipefail

if [[ -f ".cargo/config" ]]; then
    rm .cargo/config
elif [[ ! -d ".cargo" ]]; then
    mkdir .cargo
fi

# Clean any cache from cargo registry:
rm -rf $HOME/.cargo/registry

echo "[target.$TARGET]" > .cargo/config
echo "linker= \"$CC\"" >> .cargo/config

# Install clippy, according to:
# https://internals.rust-lang.org/t/clippy-is-available-as-a-rustup-component/7967
rustup update
rustup component add clippy-preview

cat .cargo/config

travis/trusty/install-capnp.sh
