#!/bin/bash

set -e

export TSUKUYOMI_DENY_WARNINGS=true

rustc --version
cargo --version

if cargo fmt --version >/dev/null 2>&1; then
    cargo fmt -- --check
fi

if cargo clippy --version >/dev/null 2>&1; then
    cargo clippy --all-features --all-targets
    cargo clippy --all-features --all-targets -p tsukuyomi-internal
    cargo clippy --all-features --all-targets -p tsukuyomi-internal-macros
fi

cargo test
cargo test --all-features
cargo test --no-default-features
cargo test -p tsukuyomi-internal
cargo test -p tsukuyomi-internal --all-features
cargo test -p tsukuyomi-internal-macros
cargo test -p doctest
