# AGENTS.md

This file contains instructions for coding agents working in this repository.

- Repository: <https://github.com/graelo/openfan-rs>
- Prefer `gh` for GitHub operations.
- Do not mention an agent or assistant in issues, pull requests, comments, or
  commit messages.
- Do not expose private local information, including machine-specific paths.

## Project

OpenFAN is a Rust workspace for controlling OpenFAN fan-controller hardware.
It provides a REST API daemon and a command-line client:

- `openfan-core`: shared types, API models, configuration, and error types.
- `openfan-hardware`: serial transport and fan-controller protocol logic.
- `openfand`: REST API server for hardware control and configuration.
- `openfanctl`: CLI client for the server, with table and JSON output.

The workspace uses Rust edition 2024 and supports the OpenFAN Standard board
and custom boards with 1 to 16 fans. Hardware is detected automatically where
possible; mock mode is available for development and testing.

## Architecture

1. `openfanctl` parses commands and sends HTTP requests to `openfand`.
2. `openfand` validates requests, manages configuration, and exposes the REST
   API under `/api/v0/`.
3. `openfan-hardware` communicates with controllers over USB serial.
4. `openfan-core` supplies the shared board, configuration, API, and error
   models used by the other crates.

Important areas:

- `openfan-core/src/board.rs`: board types, fan counts, and board metadata.
- `openfan-core/src/config/`: static configuration and mutable profiles,
  aliases, zones, curves, and CFM mappings.
- `openfan-hardware/src/`: serial transport and fan-controller protocol.
- `openfand/src/api/`: REST routes, handlers, and application state.
- `openfanctl/src/cli/`: command definitions and command handlers.
- `openfanctl/tests/`: integration and end-to-end tests.

## Verification

The `Makefile` is the canonical definition of local build and verification
tasks. **Read it before choosing or running verification commands**; do not
duplicate command implementations here. `make help` lists every target.

The primary targets are:

- `make check`: formatting, linting, and the full test suite.
- `make check-all`: `check` plus dependency, commit, Markdown, manpage, and
  GitHub Actions security checks.
- `make fix`: automatic rustfmt and Clippy fixes.
- `make md`: Markdown linting using `rumdl.toml`.
- `make man`: linting for both roff manpages.
- `make ci-security`: Poutine and Zizmor GitHub Actions scans.

The targets use locked Cargo dependency resolution where applicable and assume
external tools such as `cargo-nextest`, `cargo-deny`, `cargo-pants`, `convco`,
`poutine`, `zizmor`, `rumdl`, `mandoc`, and `cargo-llvm-cov` are installed.
The complete test sequence is implemented in `ci/test_full.sh`.

For focused tests, use `cargo nextest run -p <package>` or a specific test
name. Doctests are run separately with `cargo test --doc --workspace`.

## Documentation and releases

Keep user-facing documentation synchronized with behavior:

- Update `README.md` and `docs/TUTORIAL.md` when user-visible behavior or API
  usage changes.
- Update `man/openfand.1` or `man/openfanctl.1` when changing a CLI option,
  default, command, or exit status. Lint them with `make man` and preview them
  with `mandoc` as described in `CONTRIBUTING.md`.
- Update the `.TH` version and date in both manpages for releases.
- Update `Cargo.toml`, `Cargo.lock`, and `CHANGELOG.md` for releases.
- Commit messages must follow `.convco`; use `make commits` to check them.

Preserve the workspace structure, configuration paths, mock-mode behavior, and
`--locked` behavior in Cargo commands that resolve dependencies.
