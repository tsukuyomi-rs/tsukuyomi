#!/bin/bash

export TSUKUYOMI_DENY_WARNINGS=true

set -ex

rustc --version
cargo --version

if cargo fmt --version >/dev/null 2>&1; then
    cargo fmt -- --check
fi

if cargo clippy --version >/dev/null 2>&1; then
    cargo clippy --all-features --all-targets
    cargo clippy --all-features --all-targets -p tsukuyomi-internal-core
    cargo clippy --all-features --all-targets -p tsukuyomi-internal-macros
    cargo clippy --all-features --all-targets -p tsukuyomi-internal-runtime

    cargo clippy --all-features --all-targets -p tsukuyomi-toolkit
fi

cargo test -p tsukuyomi-internal-core
cargo test -p tsukuyomi-internal-core --all-features
cargo test -p tsukuyomi-internal-macros
cargo test -p tsukuyomi-internal-runtime
cargo test -p tsukuyomi-internal-runtime --all-features

cargo test
cargo test --all-features
cargo test --no-default-features

cargo test -p tsukuyomi-toolkit
cargo test -p doctest
