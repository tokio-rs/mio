
#!/usr/bin/env sh

# Builds and runs tests for a particular target passed as an argument to this
# script.

set -ex

TARGET="${1}"

echo "~~~~~~~~~~~~~~~~~~~~~~~~~"
echo $HOME
echo "~~~~~~~~~~~~~~~~~~~~~~~~~"

cargo test --no-default-features --target "${TARGET}"

cargo test --target "${TARGET}"
