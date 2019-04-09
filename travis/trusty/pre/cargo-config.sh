#!/usr/bin/env bash

set -eux -o pipefail

if [[ -f ".cargo/config" ]]; then
    rm .cargo/config
elif [[ ! -d ".cargo" ]]; then
    mkdir .cargo
fi

echo "[target.$TARGET]" > .cargo/config
echo "linker= \"$CC\"" >> .cargo/config

rustup update
rustup component add clippy-preview rustfmt

cat .cargo/config
