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

# build the workspace
cargo build --locked --workspace

# unit + integration tests across the workspace
cargo nextest run --locked $NEXTEST_PROFILE --workspace

# doc tests (not supported by nextest)
cargo test --locked --doc --workspace

# CLI smoke tests (release binaries). CARGO_BUILD_TARGET (set in the compat
# matrix) redirects output to target/<target>/release; Git Bash on Windows
# reports OSTYPE=msys.
cargo build --locked --release --workspace

BIN_DIR="target/${CARGO_BUILD_TARGET:+${CARGO_BUILD_TARGET}/}release"
EXT=""
case "${OSTYPE:-}" in
  msys*|cygwin*) EXT=".exe" ;;
esac
"${BIN_DIR}/openfanctl${EXT}" --help
"${BIN_DIR}/openfand${EXT}" --help
