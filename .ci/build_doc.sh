#!/bin/bash

set -ex

cargo doc --all-features --no-deps

cd toolkit
cargo doc --all-features --no-deps -p tsukuyomi-toolkit
cd ..

rm -f target/doc/.lock

echo '<meta http-equiv="refresh" content="0;URL=tsukuyomi/index.html">' > target/doc/index.html
