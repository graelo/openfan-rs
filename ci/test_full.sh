#!/bin/bash

set -e

CRATE=openfan
MSRV=1.95

get_rust_version() {
  local array=($(rustc --version));
  echo "${array[1]}";
  return 0;
}
RUST_VERSION=$(get_rust_version)

check_version() {
  IFS=. read -ra rust <<< "$RUST_VERSION"
  IFS=. read -ra want <<< "$1"
  [[ "${rust[0]}" -gt "${want[0]}" ||
   ( "${rust[0]}" -eq "${want[0]}" &&
     "${rust[1]}" -ge "${want[1]}" )
  ]]
}

echo "Testing $CRATE on rustc $RUST_VERSION"
if ! check_version $MSRV ; then
  echo "The minimum for $CRATE is rustc $MSRV"
  exit 1
fi

NEXTEST_PROFILE=""
if [ -n "$CI" ]; then
  NEXTEST_PROFILE="--profile ci"
fi

set -x

# build the workspace (dev profile)
cargo build --locked --workspace

# Point the openfanctl e2e tests at the binaries we just built.
# CARGO_BUILD_TARGET (set in the compat matrix) redirects build output to
# target/<target>/{debug,release}; without these exports the e2e tests
# look for target/debug/{openfand,openfanctl} and fail. Paths must be
# absolute — nextest sets CWD to the test's package directory, not the
# workspace root.
DEBUG_DIR="$PWD/target/${CARGO_BUILD_TARGET:+${CARGO_BUILD_TARGET}/}debug"
export OPENFAND_BINARY="${DEBUG_DIR}/openfand"
export OPENFANCTL_BINARY="${DEBUG_DIR}/openfanctl"

# unit + integration tests across the workspace
cargo nextest run --locked $NEXTEST_PROFILE --workspace

# doc tests (not supported by nextest)
cargo test --locked --doc --workspace

# CLI smoke tests (release binaries).
cargo build --locked --release --workspace

RELEASE_DIR="target/${CARGO_BUILD_TARGET:+${CARGO_BUILD_TARGET}/}release"
"${RELEASE_DIR}/openfanctl" --help
"${RELEASE_DIR}/openfand" --help
