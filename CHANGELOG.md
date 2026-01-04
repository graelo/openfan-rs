# Changelog

All notable changes to the OpenFAN Controller project will be documented in
this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- **CONTRIBUTING.md**: Developer onboarding documentation with development setup,
  testing guide, commit conventions, and PR process
- Added specific error variants for better error handling: `ZoneNotFound`,
  `CurveNotFound`, `CfmMappingNotFound`
- **Device reconnection support**: Automatic reconnection when hardware
  disconnects (USB unplug, power cycle)
  - Configurable exponential backoff retry strategy via `[reconnect]` section
    in config.toml
  - Automatic PWM state restoration after successful reconnection
  - Background heartbeat monitoring for connection health
  - New error variants: `DeviceDisconnected`, `Reconnecting`,
    `ReconnectionFailed`
  - Enhanced `/api/v0/info` endpoint with connection status fields
  - New `POST /api/v0/reconnect` endpoint for manual reconnection
  - HTTP 503 status for operations during disconnect/reconnection
- **Custom board support**: Added `Custom` board type for DIY/modified USB
  boards
  - Use `--board custom:N` where N is fan count (1-16)
  - Use `--device /dev/ttyACM0` to specify the serial device directly
  - Enables extensibility for community-built hardware
- **Multi-controller support**: Manage multiple fan controllers simultaneously
  - Configure multiple controllers via `[[controllers]]` array in config.toml
  - CLI `--controller` (`-c`) flag for controller-specific commands
  - New `openfanctl controllers` command to list all controllers
  - New `openfanctl controller info <id>` and `controller reconnect <id>` subcommands
  - Zones now support cross-controller fan grouping with `controller:fan_id` format
  - New API endpoints: `/api/v0/controllers`, `/api/v0/controller/{id}/info`,
    `/api/v0/controller/{id}/reconnect`
  - Backward compatible: single-controller setups work unchanged
- **Code quality improvements**:
  - `BoardType` enum now uses serde with string format (`"standard"`, `"custom:4"`)
  - Builder pattern for `ControllerEntry` construction
  - Removed duplicate type definitions across crates
  - Enhanced async method documentation with proper imperative mood
  - Minimized imports in openfanctl for better maintainability
  - Improved test coverage (183 integration tests, 75.21% code coverage)
    - Added integration tests for edge cases in fan handlers (non-numeric IDs, missing controllers, PWM clamping)
    - Added error conversion and Display trait tests in openfan-core
  - Version strings now use `env!("CARGO_PKG_VERSION")` for consistency
    - Replaced hardcoded version strings in production code
    - Test fixtures also use CARGO_PKG_VERSION with "-test" suffix for mock server
- **Module rename**: Renamed `openfand/src/hardware/` to `openfand/src/controllers/` for better alignment with multi-controller architecture

### Changed

- **Configuration format**: Replaced `config.yaml` with `config.toml` for all
  deployments
- **Simplified configuration**: Removed `[hardware]` section from static config
  - hardware is now auto-detected via USB VID/PID
- **Config structure**: Static config now only contains `server` settings and
  `data_dir` path
- Removed `Sync` bound from `SerialTransport` trait for better flexibility with
  async mock implementations
- Updated all deployment files (Dockerfile, docker-compose.yml, systemd
  service, install scripts) to use `config.toml`

### Removed

- **OpenFAN Micro placeholder**: Removed incorrect USB serial assumptions for
  OpenFAN Micro board (it uses WiFi, not USB serial)

### Fixed

- Integration tests now run correctly in CI environment
- Removed random failure simulation from test utilities that was causing flaky
  CI tests
- **CI coverage workflow**: Fixed `cargo-tarpaulin` execution by adding
  `--skip-clean` flag (tarpaulin 0.35.0 cleans by default, breaking E2E tests)

## [0.1.0] - 2024-12-31

### Added

- Initial release of OpenFAN Controller
- REST API server (`openfand`) with 16 endpoints
- CLI tool (`openfanctl`) with comprehensive subcommands
- Support for OpenFAN Standard (10-fan controller)
- Fan control via PWM (0-100%) or RPM target (500-9000) modes
- Profile management (save and apply fan configurations)
- Alias management (human-readable fan names)
- Zone management (group fans for coordinated control)
- Thermal curves (temperature-to-PWM mappings with linear interpolation)
- CFM mappings (display airflow estimates based on fan specs)
- Auto-detection of hardware via USB VID/PID
- Mock mode for testing without hardware
- Docker support with multi-arch builds (amd64/arm64)
- XDG-compliant configuration system
- Systemd service integration
- Shell completion for bash/zsh/fish
- 168 tests with comprehensive coverage
- Comprehensive documentation (README, TUTORIAL, architecture docs)

### Board Support

- OpenFAN Standard: 10 fans, USB VID 0x2E8A, PID 0x000A
- Custom boards: 1-16 fans, user-defined USB identifiers (use `--board
  custom:N`)

### API Endpoints

- `/api/v0/info` - Server and board information
- `/api/v0/fan/status` - Fan status (RPM and PWM)
- `/api/v0/fan/{id}/pwm` - Set fan PWM
- `/api/v0/fan/{id}/rpm` - Set fan RPM target
- `/api/v0/profiles/*` - Profile CRUD operations
- `/api/v0/alias/*` - Alias CRUD operations
- `/api/v0/zones/*` - Zone CRUD operations
- `/api/v0/curves/*` - Thermal curve CRUD operations
- `/api/v0/cfm/*` - CFM mapping CRUD operations

### Configuration

- Static config: `~/.config/openfan/config.toml` (XDG) or
  `/etc/openfan/config.toml` (system)
- Mutable data: `~/.local/share/openfan/` (XDG) or `/var/lib/openfan/` (system)
- Separate files for aliases, profiles, zones, thermal curves, and CFM mappings

## [0.0.1] - 2024-10-08

### Added

- Initial project setup with workspace structure
- Basic serial communication with hardware
- Simple fan control (PWM only)
- Basic REST API endpoints

---

[Unreleased]: https://github.com/graelo/openfan-rs/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/graelo/openfan-rs/releases/tag/v0.1.0
[0.0.1]: https://github.com/graelo/openfan-rs/releases/tag/v0.0.1
