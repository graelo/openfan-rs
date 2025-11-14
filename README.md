# OpenFAN Controller - Rust Implementation

A high-performance Rust implementation of the OpenFAN Controller, providing both an API server and CLI tool for managing fan hardware.

## Project Structure

This workspace contains three crates:

```
openfan/
├── openfan-core/          # Shared library
│   └── src/
│       ├── lib.rs         # Module exports
│       ├── error.rs       # Error types
│       └── types.rs       # Core types and API models
openfand/             # API server crate (binary: openfand)
│   └── src/
│       └── main.rs        # Server entry point
openfanctl/            # CLI tool crate (binary: openfanctl)
    └── src/
        └── main.rs        # CLI entry point
```

## Crates

### `openfan-core`

Shared library containing:
- **Types**: `FanProfile`, `Config`, `ServerConfig`, `HardwareConfig`
- **API Models**: `ApiResponse<T>`, `FanStatus`, `SystemInfo`
- **Error Types**: `OpenFanError`, `Result<T>`
- **Constants**: `MAX_FANS`, `ControlMode`

This crate is used by both the server and CLI to ensure type safety across components.

### `openfand` (binary: openfand)

REST API server that:
- Communicates with fan hardware via serial port
- Exposes 16 API endpoints for fan control
- Manages configuration (YAML)
- Runs as a systemd service

**Status**: Phase 3 complete (REST API). CLI implementation pending (Phase 4).

### `openfanctl` (binary: openfanctl)

Command-line interface that:
- Provides git-style commands (`openfanctl status`, `openfanctl fan set`, etc.)
- Communicates with the API server via HTTP
- Supports table and JSON output formats
- Generates shell completion scripts

**Status**: Phase 1 complete (skeleton). Command implementation pending (Phase 4).

## Building

Build all crates:
```bash
cargo build
```

Build specific crate:
```bash
cargo build -p openfan-core
cargo build -p openfand
cargo build -p openfanctl
```

Build release binaries:
```bash
cargo build --release
```

## Testing

Run all tests:
```bash
cargo test --workspace
```

Run tests for specific crate:
```bash
cargo test -p openfan-core
```

Run end-to-end tests (requires server startup):
```bash
cargo test --test e2e_integration_tests
```

Run unit tests only:
```bash
cargo test --lib
```

## Running

### Server

```bash
# Development (with mock hardware)
cargo run -p openfand --bin openfand -- --mock

# Production (requires hardware)
cargo run -p openfand --bin openfand

# With options
cargo run -p openfand --bin openfand -- --config config.yaml --port 3000 --verbose --mock

# Release binary
./target/release/openfand --mock
```

### CLI

```bash
# Development
cargo run -p openfanctl --bin openfanctl -- status

# With options
cargo run -p openfanctl --bin openfanctl -- --server http://localhost:3000 status

# Show help
cargo run -p openfanctl --bin openfanctl -- --help

# Generate shell completion
cargo run -p openfanctl --bin openfanctl -- completion bash > openfanctl.bash
```

## CLI Commands

```bash
openfanctl info                          # System information
openfanctl status                        # Fan status
openfanctl fan set 0 --pwm 50           # Set fan PWM
openfanctl fan set 0 --rpm 1000         # Set fan RPM
openfanctl fan rpm 0                    # Get fan RPM
openfanctl fan pwm 0                    # Get fan PWM
openfanctl profile list                 # List profiles
openfanctl profile apply "Gaming"       # Apply profile
openfanctl profile add "Custom" pwm 50,50,50,50,50,50,50,50,50,50
openfanctl profile remove "Custom"      # Remove profile
openfanctl alias list                   # List aliases
openfanctl alias get 0                  # Get alias
openfanctl alias set 0 "CPU Fan"        # Set alias
```

## Development Status

### ✅ Phase 1: Complete (Workspace & Core)

- [x] Workspace structure created
- [x] `openfan-core` implemented
  - [x] Core types and models
  - [x] Error handling
  - [x] API response structures
  - [x] Tests passing (4/4)
- [x] `openfand` skeleton
  - [x] CLI argument parsing
  - [x] Logging infrastructure
  - [x] Dependencies configured
- [x] `openfanctl` skeleton
  - [x] Command structure (clap)
  - [x] All command definitions
  - [x] Shell completion support
  - [x] Dependencies configured

### ✅ Phase 2: Complete (Hardware Layer)

- [x] Configuration loading/saving (YAML)
- [x] Serial driver implementation (async tokio-serial)
- [x] Hardware communication protocol (command/response)
- [x] Fan commander implementation (all commands)
- [x] Device auto-detection (VID:0x2E8A, PID:0x000A)
- [x] Mock mode fallback (when hardware not connected)
- [x] Tests passing (9/9)

### ✅ Phase 3: Complete (Server API)

- [x] Axum router setup with middleware stack
- [x] 16 API endpoint handlers (profiles, fans, aliases, info)
- [x] CORS middleware for cross-origin requests
- [x] Request validation and error handling
- [x] JSON API responses with proper status codes
- [x] Graceful shutdown signal handling
- [x] Mock mode operation (without hardware)
- [x] Tests passing (22/22)

### ✅ Phase 4: Complete (CLI Implementation)

- [x] HTTP client wrapper with retry logic
- [x] All 16 command implementations
- [x] Output formatting (table/JSON with colors)
- [x] CLI configuration management
- [x] Error handling and validation
- [x] Shell completion support
- [x] Tests passing (28/28)

### ✅ Phase 5: Complete (Testing)

- [x] Unit tests (28 tests passing)
- [x] Integration tests (11 tests passing)
- [x] End-to-end tests (10 E2E tests passing)
- [x] Mock server testing infrastructure
- [x] Server/CLI integration testing
- [x] Error condition testing
- [x] JSON/Table output validation

### 🚧 Phase 6: Deployment (Next)

- [ ] Release builds and optimization
- [ ] systemd service file
- [ ] Installation scripts
- [ ] Packaging (deb/rpm)
- [ ] CI/CD pipeline

## Dependencies

Key dependencies:
- **axum 0.7**: Web framework (server)
- **tokio**: Async runtime
- **tokio-serial**: Serial communication
- **clap 4**: CLI argument parsing
- **reqwest**: HTTP client (CLI)
- **serde**: Serialization
- **tracing**: Logging
- **tabled**: Pretty tables (CLI)

## Configuration

Configuration file format (YAML):
```yaml
server:
  hostname: localhost
  port: 3000
  communication_timeout: 1

hardware:
  hostname: localhost
  port: 3000
  communication_timeout: 1

fan_profiles:
  "50% PWM":
    type: pwm
    values: [50, 50, 50, 50, 50, 50, 50, 50, 50, 50]
  "100% PWM":
    type: pwm
    values: [100, 100, 100, 100, 100, 100, 100, 100, 100, 100]

fan_aliases:
  0: "Fan #1"
  1: "Fan #2"
  # ... up to 9
```

## Architecture

```
┌─────────────────┐          ┌───────────────┐
│ openfanctl      │          │  openfand     │
│ (HTTP Client)   │◄────────►│  (REST API)   │
└─────────────────┘   HTTP   │               │
                              │  ┌────────────┐  │
        ┌─────────────────────┤  │ Hardware   │  │
        │  openfan-core       │  │ Layer      │  │
        │  (Shared Types)     │  └──────┬─────┘  │
        └─────────────────────┘         │        │
                                        │ Serial │
                                        ▼        │
                                  ┌──────────┐   │
                                  │ Fan HW   │   │
                                  └──────────┘   │
                                                 └───
```

## Documentation

For complete implementation details, see:
- `../conversion/RUST-IMPLEMENTATION-ROADMAP.md` - Implementation plan
- `../conversion/PRD-RUST-CONVERSION.md` - Product requirements
- `../conversion/TECHNICAL-DESIGN-RUST.md` - Technical design
- `../conversion/CLI-DESIGN.md` - CLI architecture

## License

MIT

## Version

1.0.0 (Phase 5 Complete - Full Implementation with E2E Testing)

## Testing Status

- **Unit Tests**: 28/28 passing across all crates
- **Integration Tests**: 11/11 passing (CLI validation)
- **End-to-End Tests**: 10/10 passing (full server+CLI integration)
- **Total Test Coverage**: 49 tests, all passing

### Test Types

1. **Unit Tests**: Core functionality, API models, error handling
2. **Integration Tests**: CLI client validation without server
3. **End-to-End Tests**: Full stack testing with real server/CLI communication
   - Server startup/shutdown in mock mode
   - Fan control operations (PWM/RPM)
   - Profile management (add/remove/apply)
   - Alias management (get/set/list)
   - Error handling and validation
   - JSON and table output formats
   - Connection failure scenarios
