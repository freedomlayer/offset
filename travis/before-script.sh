#!/usr/bin/env bash

set -eux -o pipefail

printenv

if [[ -f ".cargo/config" ]]; then
    rm .cargo/config
elif [[ ! -d ".cargo" ]]; then
    mkdir .cargo
fi

echo "[target.$TARGET]" > .cargo/config
echo "linker= \"$CC\"" >> .cargo/config

cat .cargo/config

travis/install-capnp.sh
