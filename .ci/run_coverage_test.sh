#!/bin/bash

set -ex

curl -s https://codecov.io/bash -o .codecov
chmod +x .codecov

codecov() {
    local branch="${BUILD_SOURCEBRANCHNAME:-}"
    local commit="${BUILD_SOURCEVERSION:-}"
    local pr="${SYSTEM_PULLREQUEST_PULLREQUESTNUMBER:-}"
    local build="${BUILD_BUILDID:-}"
    ./.codecov -B "$branch" -C "$commit" -P "$pr" -b "$build" -K "$@"
}

tarpaulin() {
    cargo tarpaulin -v --skip-clean --out Xml "$@"
}

tarpaulin --all && codecov -n "all"
tarpaulin -p tsukuyomi-server --all-features && codecov -n "tsukuyomi-server (with all features)"
tarpaulin -p tsukuyomi-session --all-features && codecov -n "tsukuyomi-session (with all features)"
