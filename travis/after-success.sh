#!/usr/bin/env bash

CODECOV_TOKEN="97f1d206-36ff-41f6-8cc8-0b2ed8a48a2d"

if [[ "$KCOV" == "1" ]]; then
    travis/install-kcov.sh

    cargo clean
    RUSTFLAGS="-C link-dead-code" cargo test -vv --no-run --target=${TARGET}

    exes=$(find target/${TARGET}/debug -maxdepth 1 -executable -type f)
    for exe in ${exes}; do
        ${HOME}/kcov-${TARGET}/bin/kcov \
            --verify \
            --exclude-path=/usr/include \
            --include-pattern="cswitch/src,cswitch/test_utils" \
            target/kcov \
            ${exe}
    done

    bash <(curl -s https://codecov.io/bash)
fi
