#!/usr/bin/env bash

export ANDROID_HOME="${HOME}/android-sdk"
export ANDROID_NDK_HOME="${ANDROID_HOME}/ndk-bundle"
export PATH=$PATH:"${ANDROID_HOME}/tools/bin:${ANDROID_NDK_HOME}/toolchains/llvm/prebuilt/linux-x86_64/bin"

# Make sure that directories exist:
mkdir -p $ANDROID_HOME
mkdir -p ${HOME}/android-sdk-dl
