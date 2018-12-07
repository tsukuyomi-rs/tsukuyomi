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
    cargo tarpaulin -v --skip-clean --ignore-tests --out Xml "$@"
}

tarpaulin --all --exclude example-diesel && codecov -n "all" # example-diesel reaches to the type-length limit...
tarpaulin -p tsukuyomi --all-features && codecov -n "tsukuyomi (with all features)"
#tarpaulin -p tsukuyomi-session --all-features && codecov -n "tsukuyomi-session (with all features)"
