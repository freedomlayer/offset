#!/usr/bin/env bash

CAPNP_INSTALL_PREFIX="${HOME}/capnp"

CAPNP_VERSION="0.7.0"

if [ ! -d "$HOME/capnp/lib" ]; then
  curl -L https://capnproto.org/capnproto-c++-${CAPNP_VERSION}.tar.gz | tar -zxf -
  pushd capnproto-c++-${CAPNP_VERSION}
  ./configure --prefix=${CAPNP_INSTALL_PREFIX}
  make check -j2
  sudo make install
  popd
fi

sudo ln -s ${CAPNP_INSTALL_PREFIX}/bin/capnp /usr/local/bin/capnp
