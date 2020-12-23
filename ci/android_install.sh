#!/usr/bin/env bash

# See: https://vaneyckt.io/posts/safer_bash_scripts_with_set_euxo_pipefail/
set -eux -o pipefail

# Android versions:
# See: https://developer.android.com/studio/index.html 
# (commandline-tools)
export ANDROID_SDK_VERSION=6200805
export ANDROID_BUILD_TOOLS_VERSION="26.0.2"
export ANDROID_VERSION=26

# Download and unzip the Android SDK tools (if not already there thanks to the cache mechanism)
# Latest version available here: https://developer.android.com/studio/#command-tools
if test ! -e $HOME/android-sdk-dl/commandlinetools.zip ; then 
  curl "https://dl.google.com/android/repository/commandlinetools-linux-${ANDROID_SDK_VERSION}_latest.zip" > "$HOME/android-sdk-dl/commandlinetools.zip" ; 
fi
unzip -qq -n $HOME/android-sdk-dl/commandlinetools.zip -d $ANDROID_HOME;

# Create repositories.cfg ahead of time, see: 
# https://stackoverflow.com/questions/43433542/stuck-at-android-repositories-cfg-could-not-be-loaded
mkdir -p $HOME/.android && touch $HOME/.android/repositories.cfg

# Accept the license:
# See https://stackoverflow.com/a/57797492 for (yes || true) trick explanation:
(yes || true) | $ANDROID_HOME/tools/bin/sdkmanager --sdk_root=${ANDROID_HOME} --licenses > /dev/null

# Install or update Android SDK components (will not do anything if already up to date thanks to the cache mechanism)
$ANDROID_HOME/tools/bin/sdkmanager --sdk_root=${ANDROID_HOME} \
        "platforms;android-${ANDROID_VERSION}" "build-tools;${ANDROID_BUILD_TOOLS_VERSION}" 'ndk-bundle' > /dev/null
# Add rust targets:
rustup target add aarch64-linux-android armv7-linux-androideabi i686-linux-android

# Create $HOME/.cargo/config (from template):
cat ci/cargo_config_template | envsubst > $HOME/.cargo/config

# Add symlinks for clang compilers:
cd "${ANDROID_NDK_HOME}/toolchains/llvm/prebuilt/linux-x86_64/bin"
ln -sf "./aarch64-linux-android${ANDROID_VERSION}-clang" aarch64-linux-android-clang
ln -sf "./armv7a-linux-androideabi${ANDROID_VERSION}-clang" arm-linux-androideabi-clang
ln -sf "./i686-linux-android${ANDROID_VERSION}-clang" i686-linux-android-clang
cd -

# Make sure rust stable is installed
rustup install stable
