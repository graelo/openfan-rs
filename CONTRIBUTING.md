# Contributing to OpenFAN

Thank you for your interest in contributing to OpenFAN.

## Development Environment Setup

Install the stable Rust toolchain with [rustup](https://rustup.rs/). Docker is
optional and is only needed for container builds and testing.

```bash
git clone https://github.com/graelo/openfan-rs.git
cd openfan-rs
make build
```

The workspace contains these crates:

```text
openfan-rs/
├── openfan-core/      # Shared types, API models, and configuration
├── openfan-hardware/  # Serial communication and hardware protocol
├── openfand/          # REST API server (Axum)
└── openfanctl/        # CLI client (clap + reqwest)
```

## Verification

The `Makefile` is the canonical definition of local build and verification
tasks. Run `make help` to list every target.

The primary targets are:

- `make check`: formatting, linting, and the full test suite.
- `make check-all`: `check` plus dependency, commit, documentation, and CI
  security checks.
- `make fix`: automatic rustfmt and Clippy fixes.
- `make md`: Markdown linting using `rumdl.toml`.
- `make man`: roff manpage linting.
- `make coverage`: HTML coverage using `cargo-llvm-cov`.

The verification targets assume that external tools such as
`cargo-nextest`, `cargo-deny`, `cargo-pants`, `convco`, `poutine`, `zizmor`,
`rumdl`, `mandoc`, and `cargo-llvm-cov` are installed locally.

For focused Rust tests, use nextest directly:

```bash
cargo nextest run -p openfan-core
cargo nextest run --test e2e_integration_tests -p openfanctl
cargo test --doc --workspace
```

The complete test sequence, including the release-binary smoke tests, is in
[`ci/test_full.sh`](ci/test_full.sh).

## Running the Project

### Mock Mode

```bash
# Start the server without hardware
cargo run -p openfand -- --mock --board standard

# In another terminal
cargo run -p openfanctl -- status
```

### With Hardware

```bash
# Auto-detect an OpenFAN Standard board
cargo run -p openfand

# Specify a custom board and serial device
cargo run -p openfand -- --device /dev/ttyACM0 --board custom:4
```

## Docker Development

```bash
make docker
make docker-multiplatform

docker run -p 3000:3000 openfan:latest openfand --mock --board standard
```

## Manpages

Manpages live in `man/` as roff source: `openfand.1` and `openfanctl.1`.

Preview them with:

```bash
mandoc man/openfand.1 | less
mandoc man/openfanctl.1 | less
```

Lint both with:

```bash
make man
```

Update the relevant manpage when adding, removing, or renaming a CLI option,
changing a default, or changing a command. Update the `.TH` version and date
when preparing a release.

## Code Style

- Use Rust 2024 and stable Rust.
- Run `make fmt` before committing.
- Run `make lint` and address all warnings.
- Add documentation for public APIs.
- Follow idiomatic Rust naming and module structure.

## Commit Messages

Use [Conventional Commits](https://www.conventionalcommits.org/):

```text
<type>: <description>
```

Common types include `feat`, `fix`, `docs`, `refactor`, `test`, and `chore`.
Run `make commits` to check commit messages against `.convco`.

## Pull Requests

1. Create a focused feature or fix branch.
2. Update user-facing documentation when behavior changes.
3. Run `make check` and, before opening a PR, `make check-all` when all local
   audit tools are available.
4. Commit with a Conventional Commit message.
5. Open a pull request against `main`.

## Reporting Bugs

Use the [GitHub Issues](https://github.com/graelo/openfan-rs/issues) tracker.
Include reproduction steps, expected and actual behavior, environment details,
and relevant logs.

## License

By contributing, you agree that your contributions will be licensed under the
MIT License or Apache License 2.0, at the project's option.
