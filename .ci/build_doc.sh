#!/bin/bash

set -ex

cargo doc --no-deps -p juniper
cargo doc --no-deps -p tungstenite -p tokio-tungstenite
cargo doc --no-deps -p tokio-rustls

cargo doc --no-deps -p tsukuyomi-service
cargo doc --no-deps -p tsukuyomi-macros
cargo doc --no-deps -p tsukuyomi --all-features
cargo doc --no-deps -p tsukuyomi-server

cargo doc --no-deps -p tsukuyomi-askama
cargo doc --no-deps -p tsukuyomi-cors
cargo doc --no-deps -p tsukuyomi-juniper
cargo doc --no-deps -p tsukuyomi-session --all-features
cargo doc --no-deps -p tsukuyomi-tungstenite

rm -f target/doc/.lock

echo '<meta http-equiv="refresh" content="0;URL=tsukuyomi/index.html">' > target/doc/index.html
