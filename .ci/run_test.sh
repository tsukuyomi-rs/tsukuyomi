#!/bin/bash

export TSUKUYOMI_DENY_WARNINGS=true

set -ex

rustc --version
cargo --version

if cargo fmt --version >/dev/null 2>&1; then
    cargo fmt -- --check
fi

cargo test --all
cargo test -p doctest

cargo test -p tsukuyomi --all-features
cargo test -p tsukuyomi --no-default-features
cargo test -p tsukuyomi-core --all-features
cargo test -p tsukuyomi-server --all-features

cargo test -p tsukuyomi-session --all-features
cargo test -p tsukuyomi-session --no-default-features

if cargo clippy --version >/dev/null 2>&1; then
    cargo clippy --all

    cargo clippy -p tsukuyomi --all-features --all-targets
    cargo clippy -p tsukuyomi-core --all-features --all-targets
    cargo clippy -p tsukuyomi-server --all-features --all-targets
    cargo clippy -p tsukuyomi-session --all-features --all-targets
fi
