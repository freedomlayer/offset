#!/usr/bin/env bash

CAPNP_INSTALL_PREFIX="${HOME}/capnp"

if [ ! -d "$HOME/capnp/lib" ]; then
  curl -L https://capnproto.org/capnproto-c++-0.6.1.tar.gz | tar -zxf -
  pushd capnproto-c++-0.6.1
  ./configure --prefix=${CAPNP_INSTALL_PREFIX}
  make check -j2
  sudo make install
  popd
fi

sudo ln -s ${CAPNP_INSTALL_PREFIX}/bin/capnp /usr/local/bin/capnp

