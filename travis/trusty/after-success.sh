#!/usr/bin/env bash

# CODECOV_TOKEN Must be set at this point.

if [[ "$KCOV" == "1" ]]; then
    travis/trusty/install-kcov.sh

    cargo clean
    RUSTFLAGS="-C link-dead-code" cargo test -v --no-run --target=${TARGET}

    exes=$(find target/${TARGET}/debug -maxdepth 1 -executable -type f)
    for exe in ${exes}; do
        ${HOME}/kcov-${TARGET}/bin/kcov \
            --verify \
            --exclude-path=/usr/include \
            --include-pattern="cswitch/src" \
            target/kcov \
            ${exe}
    done

    bash <(curl -s https://codecov.io/bash) -t ${CODECOV_TOKEN}
fi
