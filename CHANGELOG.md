# Changelog

All notable changes to the OpenFAN Controller project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- Added specific error variants for better error handling: `ZoneNotFound`, `CurveNotFound`, `CfmMappingNotFound`

### Changed
- Removed `Sync` bound from `SerialTransport` trait for better flexibility with async mock implementations

### Fixed
- Integration tests now run correctly in CI environment

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
- OpenFAN Micro: Planned (1 fan, USB VID 0x2E8A, PID 0x000B)

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
- Static config: `~/.config/openfan/config.toml` (XDG) or `/etc/openfan/config.toml` (system)
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
