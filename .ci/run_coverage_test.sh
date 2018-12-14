#!/bin/bash

DIR="$(cd $(dirname $BASH_SOURCE); pwd)"

set -ex

$DIR/run_kcov.py --all
$DIR/run_kcov.py -p tsukuyomi --all-features
$DIR/run_kcov.py -p tsukuyomi-session --all-features

bash <(curl -s https://codecov.io/bash) \
  -B "${BUILD_SOURCEBRANCHNAME:-}" \
  -C "${BUILD_SOURCEVERSION:-}" \
  -P "${SYSTEM_PULLREQUEST_PULLREQUESTNUMBER:-}" \
  -b "${BUILD_BUILDID:-}"\
  -K \
  -s "$DIR/../target/cov"
