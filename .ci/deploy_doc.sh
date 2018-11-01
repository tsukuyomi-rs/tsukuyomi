#!/bin/bash

set -e

BRANCH=${BUILD_SOURCEBRANCHNAME:-}
REV=$(git rev-parse --short HEAD)
GH_TOKEN=${GH_TOKEN:-}

if [[ ${BRANCH} != 'master' ]]; then
    echo "[The current branch is not master]"
    exit 0
fi

if [[ -z ${GH_TOKEN:-} ]]; then
    echo "[GH_TOKEN is not set]"
    exit 0
fi

echo "[Deploy Generated API doc to GitHub Pages]"

cd target/doc
git init
git remote add upstream "https://${GH_TOKEN}@github.com/tsukuyomi-rs/tsukuyomi.git"
git config user.name 'Yusuke Sasaki'
git config user.email 'yusuke.sasaki.nuem@gmail.com'
git add -A .
git commit -qm "Build API doc at ${REV}"

echo "[Pushing gh-pages to GitHub]"
git push -q upstream HEAD:refs/heads/gh-pages --force
