#!/usr/bin/env bash

CAPNP_INSTALL_PREFIX="${HOME}/install/capnp"

CAPNP_VERSION="0.7.0"

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
