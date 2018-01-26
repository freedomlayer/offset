#!/usr/bin/env bash

set -ex

printenv

KCOV_INSTALL_PREFIX="${HOME}/kcov-${TARGET}"
KCOV_MINIMUM_REQUIRED=${KCOV_MINIMUM_REQUIRED:-34}

# https://github.com/SimonKagstrom/kcov/blob/master/INSTALL.md
sudo apt-get install -y cmake binutils-dev libcurl4-openssl-dev \
                        zlib1g-dev libdw-dev libiberty-dev

if [[ -f "$KCOV_INSTALL_PREFIX/bin/kcov" ]]; then
    KCOV_INSTALLED_VERSION=$(${KCOV_INSTALL_PREFIX}/bin/kcov --version)
    KCOV_INSTALLED_VERSION=${KCOV_INSTALLED_VERSION#*\ }

    if (( $KCOV_INSTALLED_VERSION >= $KCOV_MINIMUM_REQUIRED )); then
        echo "Using cached kcov, version: $KCOV_INSTALLED_VERSION"
        exit 0
    else
       rm -rf "$KCOV_INSTALL_PREFIX/bin/kcov"
    fi
fi

curl -L https://github.com/SimonKagstrom/kcov/archive/v${KCOV_MINIMUM_REQUIRED}.tar.gz | tar -zxf -

pushd kcov-${KCOV_MINIMUM_REQUIRED}

mkdir build

pushd build

TARGET=${TARGET} cmake -DCMAKE_INSTALL_PREFIX:PATH="${KCOV_INSTALL_PREFIX}" ..

make
make install

${KCOV_INSTALL_PREFIX}/bin/kcov --version

popd
popd
