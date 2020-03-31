#!/usr/bin/env bash

# See: https://vaneyckt.io/posts/safer_bash_scripts_with_set_euxo_pipefail/
set -eux -o pipefail

#       if [[ -f ".cargo/config" ]]; then
#           rm .cargo/config
#       elif [[ ! -d ".cargo" ]]; then
#           mkdir .cargo
#       fi

# echo "[target.$TARGET]" > .cargo/config
# echo "linker= \"$CC\"" >> .cargo/config

rustup update
rustup component add clippy rustfmt

cat .cargo/config
