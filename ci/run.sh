#!/bin/sh

# Builds and runs tests for a particular target passed as an argument to this
# script.

set -ex

TARGET=$1

cargo build --test test --target $TARGET

# Find the file to run
TEST_FILE="$(find target/$TARGET/debug -maxdepth 1 -type f -name test-* | head -1)"

case "$TARGET" in
  arm-linux-androideabi)
    # Use the 64bit emulator
    emulator64-arm @arm-21 -no-window &
    adb wait-for-device
    adb push $TEST_FILE /data/mio-test
    adb shell /data/mio-test 2>&1 | tee /tmp/out
    grep "^test result.* 0 failed" /tmp/out
    ;;

  *)
    exit 1;
    ;;
esac
