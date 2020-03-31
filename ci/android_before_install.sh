#!/usr/bin/env bash

# See: https://vaneyckt.io/posts/safer_bash_scripts_with_set_euxo_pipefail/
set -eux -o pipefail

export ANDROID_HOME="${HOME}/android-sdk"
mkdir -p $ANDROID_HOME
mkdir -p ${HOME}/android-sdk-dl
export ANDROID_NDK_HOME="${ANDROID_HOME}/ndk-bundle"
export PATH=$PATH:"${ANDROID_HOME}/tools/bin:${ANDROID_NDK_HOME}/toolchains/llvm/prebuilt/linux-x86_64/bin"
