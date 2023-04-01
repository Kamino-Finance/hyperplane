#!/usr/bin/env bash

set -e

source ci/rust-version.sh stable
source ci/solana-version.sh install

set -x

cargo --version

cargo +"$rust_stable" install --git https://github.com/hubbleprotocol/anchor --branch "feature/token-program-constraint" anchor-cli --locked
