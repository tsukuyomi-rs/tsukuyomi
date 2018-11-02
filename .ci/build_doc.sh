#!/bin/bash

set -e

cargo doc --all-features --no-deps \
    -p askama -p failure -p tungstenite -p tokio-tungstenite -p walkdir
cargo doc --all-features --no-deps \
    -p tsukuyomi-internal -p tsukuyomi-internal-macros
cargo doc --all-features --no-deps
rm -f target/doc/.lock

echo '<meta http-equiv="refresh" content="0;URL=tsukuyomi/index.html">' > target/doc/index.html
