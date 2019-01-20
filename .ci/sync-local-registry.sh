#!/bin/bash

set -e

echo "[sync Git submodule]"
(set -x; git submodule update --init --depth=1)

echo "[check if the registry index is up to date]"
(set -x; diff -u Cargo.lock .registry-index/Cargo.lock)

echo "[replace crates-io with the local registry index]"
cat << EOF >> .cargo/config
[source.crates-io]
replace-with = "registry-index"
EOF
