# Contributing to OpenFAN

Thank you for your interest in contributing to OpenFAN! This document provides
guidelines to help you get started.

## Development Environment Setup

### Prerequisites

- **Rust**: Install via [rustup](https://rustup.rs/) (stable toolchain)
- **Git**: For version control
- **Docker** (optional): For container builds and testing

### Getting Started

```bash
# Clone the repository
git clone https://github.com/graelo/openfan-rs.git
cd openfan-rs

# Build all crates
cargo build
# Or use make
make build

# Run tests to verify setup
cargo test --workspace
# Or use make
make test
```

### Project Structure

```text
openfan-rs/
├── openfan-core/      # Shared types, API models, error types
├── openfan-hardware/  # Serial communication, hardware protocol
├── openfand/          # REST API server (Axum)
└── openfanctl/        # CLI client (clap)
```

## Running the Project

### Mock Mode (No Hardware Required)

```bash
# Start server in mock mode
cargo run -p openfand -- --mock --board standard

# In another terminal, use the CLI
cargo run -p openfanctl -- status
```

### With Hardware

```bash
# Auto-detect OpenFAN Standard board
cargo run -p openfand

# Or specify device directly for custom boards
cargo run -p openfand -- --device /dev/ttyACM0 --board custom:4
```

## Running Tests

```bash
# Run all tests (481 tests total)
cargo test --workspace
# Or use make
make test

# Run tests for a specific crate
cargo test -p openfan-core

# Run unit tests only
cargo test --lib

# Run end-to-end tests
cargo test --test e2e_integration_tests

# Run with output
cargo test --workspace -- --nocapture

# Generate code coverage report
# Note: Use --skip-clean to prevent tarpaulin from cleaning build artifacts
# (needed for E2E tests that depend on compiled binaries)
cargo tarpaulin --verbose --skip-clean -p openfand -p openfan-core -p openfan-hardware --timeout 120
```

## Docker Development

```bash
# Build Docker image with version from Cargo.toml
make docker

# Build for multiple platforms (amd64/arm64)
make docker-multiplatform

# Test in Docker mock mode
docker run -p 3000:3000 openfan:latest openfand --mock --board standard

# Using docker-compose
VERSION=$(grep '^version' Cargo.toml | head -1 | sed 's/.*"\(.*\)".*/\1/') docker-compose up
```

## Code Style Guidelines

- **Edition**: Rust 2021
- **Formatting**: Use `cargo fmt` before committing
- **Linting**: Run `cargo clippy` and address warnings
- **Style**: Follow idiomatic Rust patterns
- **Documentation**: Add doc comments for public APIs

```bash
# Format code
cargo fmt

# Run clippy
cargo clippy --workspace --all-targets
```

## Commit Message Conventions

We use [Conventional Commits](https://www.conventionalcommits.org/) format:

```text
<type>: <description>

[optional body]
```

### Types

| Type | Description |
|------|-------------|
| `feat` | New feature |
| `fix` | Bug fix |
| `docs` | Documentation changes |
| `refactor` | Code refactoring (no functional change) |
| `test` | Adding or updating tests |
| `chore` | Maintenance tasks |

### Examples

```text
feat: add PWM curve configuration
fix: correct fan speed calculation overflow
docs: update API endpoint documentation
refactor: extract board detection into separate module
test: add integration tests for profile management
chore: update dependencies
```

## Pull Request Process

1. **Fork** the repository and create a feature branch:

   ```bash
   git checkout -b feat/your-feature-name
   ```

2. **Make your changes** following the code style guidelines

3. **Test your changes**:

   ```bash
   cargo test --workspace
   cargo clippy --workspace --all-targets
   cargo fmt --check
   ```

4. **Commit** with a descriptive message following conventions

5. **Push** and open a pull request against `main`

6. **Address review feedback** if requested

### PR Guidelines

- Keep PRs focused on a single change
- Include tests for new functionality
- Update documentation if needed
- Ensure CI passes before requesting review

## Reporting Bugs

When reporting bugs, please include:

1. **Description**: Clear summary of the issue
2. **Steps to reproduce**: Minimal steps to trigger the bug
3. **Expected behavior**: What should happen
4. **Actual behavior**: What happens instead
5. **Environment**: OS, Rust version, hardware (if applicable)
6. **Logs**: Relevant error messages or debug output

Use the [GitHub Issues](https://github.com/graelo/openfan-rs/issues) tracker.

## Feature Requests

For feature requests:

1. Check existing issues to avoid duplicates
2. Describe the use case and motivation
3. Propose a solution if you have one

## Questions

For questions about the codebase or development:

- Open a [GitHub Discussion](https://github.com/graelo/openfan-rs/discussions)
- Check existing documentation in `memory-bank/docs/`

## License

By contributing, you agree that your contributions will be licensed under the
MIT License.
