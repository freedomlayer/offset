#!/usr/bin/env bash

curl -L https://capnproto.org/capnproto-c++-0.6.1.tar.gz | tar -zxf -

pushd capnproto-c++-0.6.1

./configure
make check
sudo make install

popd
