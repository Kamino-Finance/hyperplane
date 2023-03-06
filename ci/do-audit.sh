#!/usr/bin/env bash

set -e
cd "$(dirname "$0")/.."
source ./ci/rust-version.sh stable

cargo_audit_ignores=(
  # Potential segfault in the time crate
  #
  # Blocked on chrono updating `time` to >= 0.2.23
  --ignore RUSTSEC-2020-0071
  # Windows-only config corruption
  #
  # https://rustsec.org/advisories/RUSTSEC-2023-0001
  # Blocked upstream on solana >= `1.14.12`
  --ignore RUSTSEC-2023-0001
  # Windows-only config corruption
  #
  # https://rustsec.org/advisories/RUSTSEC-2023-0018
  # Race condition in proptest transitive dependency - ignored as only used in tests
  --ignore RUSTSEC-2023-0018
)
cargo +"$rust_stable" audit "${cargo_audit_ignores[@]}"
