#!/usr/bin/env bash

# CODECOV_TOKEN Must be set at this point.

# See: https://vaneyckt.io/posts/safer_bash_scripts_with_set_euxo_pipefail/
set -eux -o pipefail

exes=$(find target/${TARGET}/debug -maxdepth 1 -executable -type f)
for exe in ${exes}; do
    ${HOME}/install/kcov-${TARGET}/bin/kcov \
        --verify \
        --exclude-path=/usr/include \
        --include-pattern="components" \
        target/kcov \
        ${exe}
done

# DEBUG: Show contents of directory, to see if a report was created:
pwd
ls

# TODO: Change to something safer:
# Automatically reads from CODECOV_TOKEN environment variable:
bash <(curl -s https://codecov.io/bash)
