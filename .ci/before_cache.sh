#!/bin/bash

set -ex

# lib/bin name
rm -rfv target/debug/incremental/build_script_build-*
rm -rfv target/debug/incremental/tsukuyomi-*
rm -rfv target/debug/incremental/tsukuyomi_*
rm -rfv target/debug/incremental/test_*
rm -rfv target/debug/incremental/example_*

# package name
rm -rfv target/debug/.fingerprint/tsukuyomi-*
rm -rfv target/debug/.fingerprint/test-*
rm -rfv target/debug/.fingerprint/example-*

# package name
rm -rfv target/debug/build/tsukuyomi-*
rm -rfv target/debug/build/test-*
rm -rfv target/debug/build/example-*

# lib/bin name
rm -rfv target/debug/deps/libtsukuyomi*
rm -rfv target/debug/deps/tsukuyomi*
rm -rfv target/debug/deps/libtest_*
rm -rfv target/debug/deps/test_*
rm -rfv target/debug/deps/libexample_*
rm -rfv target/debug/deps/example_*

# lib/bin name
rm -rfv target/debug/libtsukuyomi*
rm -rfv target/debug/tsukuyomi*
rm -rfv target/debug/libtest_*
rm -rfv target/debug/test_*
rm -rfv target/debug/libexample_*
rm -rfv target/debug/example_*

cargo clean -p tsukuyomi-core
cargo clean -p tsukuyomi-macros
cargo clean -p tsukuyomi-server
cargo clean -p tsukuyomi

cargo clean -p tsukuyomi-askama
cargo clean -p tsukuyomi-askama-macros
cargo clean -p tsukuyomi-juniper
cargo clean -p tsukuyomi-fs
cargo clean -p tsukuyomi-session
cargo clean -p tsukuyomi-websocket

cargo clean -p example-basic
cargo clean -p example-diesel
cargo clean -p example-json
cargo clean -p example-juniper
cargo clean -p example-routing
cargo clean -p example-session
cargo clean -p example-staticfile
cargo clean -p example-tls
cargo clean -p example-unix-socket
cargo clean -p example-websocket

rm -rf target/.rustc_info.json
