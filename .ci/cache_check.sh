#!/bin/bash

LAST_RUSTC_VERSION=$(cat "target/rustc_version" 2>/dev/null)
CURR_RUSTC_VERSION=$(rustc --version)

if [ -z "$LAST_RUSTC_VERSION" ]; then
    echo "the cache has not been saved yet."
elif [ "$LAST_RUSTC_VERSION" != "$CURR_RUSTC_VERSION" ]; then
    echo "rustc version is changed (${LAST_RUSTC_VERSION} => ${CURR_RUSTC_VERSION})"
    (set -x; cargo clean)
fi

mkdir -p target 2>/dev/null
echo "$CURR_RUSTC_VERSION" > target/rustc_version
