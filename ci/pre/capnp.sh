#!/usr/bin/env bash

# See: https://vaneyckt.io/posts/safer_bash_scripts_with_set_euxo_pipefail/
set -eux -o pipefail

CAPNP_INSTALL_PREFIX="${HOME}/install/capnp"

CAPNP_VERSION="0.7.0"

export CC="gcc-6"
export CXX="g++-6" 
export CPPFLAGS="-std=c++14" 
export CXXFLAGS="-std=c++14"

# Build only if we don't have a cached installation:
if [ ! -d "$CAPNP_INSTALL_PREFIX/lib" ]; then
  curl -L https://capnproto.org/capnproto-c++-${CAPNP_VERSION}.tar.gz | tar -zxf -
  pushd capnproto-c++-${CAPNP_VERSION}
  ./configure --prefix=${CAPNP_INSTALL_PREFIX}
  make check -j2
  sudo make install
  popd
fi

sudo ln -s ${CAPNP_INSTALL_PREFIX}/bin/capnp /usr/local/bin/capnp
