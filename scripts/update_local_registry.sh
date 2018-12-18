#!/bin/bash

set -e

MANIFEST_DIR="$(cd $(dirname $BASH_SOURCE)/..; pwd)"

echo "[regenerate Cargo.lock...]"
cd
(set -x; cargo generate-lockfile --manifest-path=$MANIFEST_DIR/Cargo.toml)

echo "[fetch local registry...]"
(set -x; cargo local-registry --verbose -s $MANIFEST_DIR/Cargo.lock $MANIFEST_DIR/.registry-index)
(set -x; cp $MANIFEST_DIR/Cargo.lock $MANIFEST_DIR/.registry-index/Cargo.lock)
