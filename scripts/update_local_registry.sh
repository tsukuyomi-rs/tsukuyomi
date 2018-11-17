#!/bin/bash

set -e

MANIFEST_DIR="$(cd $(dirname $BASH_SOURCE)/..; pwd)"

echo "[regenerate Cargo.lock...]"
cd
(set -x; cargo generate-lockfile --manifest-path=$MANIFEST_DIR/Cargo.toml)

echo "[fetch local registry...]"
cd $MANIFEST_DIR
(set -x; cargo local-registry -s Cargo.lock .registry-index)
(set -x; cp Cargo.lock .registry-index/Cargo.lock)
