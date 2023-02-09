#!/usr/bin/env bash

set -ex
cd "$(dirname "$0")/.."
source ./ci/solana-version.sh install

cd js
yarn install
yarn lint
yarn anchor-client-gen:mainnet
yarn build
yarn start-with-test-validator
(cd ../target/deploy && mv hyperplane_production.so hyperplane.so)
SWAP_PROGRAM_OWNER_FEE_ADDRESS="HfoTxFR1Tm6kGmWgYWD6J7YHVy1UwqSULUGVLXkJqaKN" yarn start-with-test-validator
