#!/bin/bash

set -ex

cargo doc --no-deps -p walkdir
cargo doc --no-deps -p tungstenite -p tokio-tungstenite

cargo doc --no-deps -p tsukuyomi-server --all-features
cargo doc --no-deps -p tsukuyomi-macros
cargo doc --no-deps -p tsukuyomi-core  --all-features
cargo doc --no-deps -p tsukuyomi --all-features

cargo doc --no-deps -p tsukuyomi-askama
cargo doc --no-deps -p tsukuyomi-fs
cargo doc --no-deps -p tsukuyomi-session --all-features
cargo doc --no-deps -p tsukuyomi-websocket

rm -f target/doc/.lock

echo '<meta http-equiv="refresh" content="0;URL=tsukuyomi/index.html">' > target/doc/index.html
