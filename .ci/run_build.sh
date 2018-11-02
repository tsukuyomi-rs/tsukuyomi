#!/bin/bash

set -e

rustc --version
cargo --version

if cargo fmt --version >/dev/null 2>&1; then
    cargo fmt -- --check
fi

if cargo clippy --version >/dev/null 2>&1; then
    cargo clippy --all-features --all-targets
fi

cargo build --all-features --all-targets
