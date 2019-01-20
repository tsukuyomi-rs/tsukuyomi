#!/bin/bash

set -e

MANIFEST_DIR="$(cd $(dirname $BASH_SOURCE)/..; pwd)"
REGISTRY_INDEX="$(cd $MANIFEST_DIR/.registry-index; pwd)"

echo "[regenerate Cargo.lock...]"
cd
(set -x; cargo generate-lockfile --manifest-path=$MANIFEST_DIR/Cargo.toml)

echo "[remove the old files in the local registry...]"
rm -f $REGISTRY_INDEX/*.crate
rm -rf $REGISTRY_INDEX/index/
rm -f $REGISTRY_INDEX/Cargo.lock

echo "[fetch local registry...]"
(set -x; cargo local-registry --verbose -s $MANIFEST_DIR/Cargo.lock $REGISTRY_INDEX)
(set -x; cp $MANIFEST_DIR/Cargo.lock $REGISTRY_INDEX/Cargo.lock)
