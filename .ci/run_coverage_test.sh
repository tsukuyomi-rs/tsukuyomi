#!/bin/bash

DIR="$(cd $(dirname $BASH_SOURCE); pwd)"

set -ex

cargo tarpaulin --verbose --out Xml --all
bash <(curl -s https://codecov.io/bash)

cargo tarpaulin --verbose --packages tsukuyomi --all-features
bash <(curl -s https://codecov.io/bash)

# cargo tarpaulin --verbose --packages tsukuyomi-session --all-features
# bash <(curl -s https://codecov.io/bash)
