#!/bin/sh
set -eu

mkdir -p .factory/tmp .factory/logs .local/state

if [ -f package.json ]; then
  npm install
fi

if [ -f Cargo.toml ]; then
  cargo fetch
fi
