# OpenFAN Controller

[![release](https://img.shields.io/github/v/release/graelo/openfan-rs)](https://github.com/graelo/openfan-rs/releases/latest)
[![build status](https://github.com/graelo/openfan-rs/actions/workflows/ci.yml/badge.svg)](https://github.com/graelo/openfan-rs/actions/workflows/ci.yml)
[![codecov](https://codecov.io/gh/graelo/openfan-rs/branch/main/graph/badge.svg)](https://codecov.io/gh/graelo/openfan-rs)
[![rust 2021 edition](https://img.shields.io/badge/edition-2021-blue.svg)](https://doc.rust-lang.org/edition-guide/rust-2021/index.html)
[![license](https://img.shields.io/github/license/graelo/openfan-rs)](LICENSE)

A Rust-based controller for OpenFAN hardware - manage your fans via REST API or
CLI.

## Overview

OpenFAN is a fan controller system consisting of:

- **Hardware**: A microcontroller board that controls PWM fans via USB serial
- **Server** (`openfand`): REST API daemon that communicates with the hardware
- **CLI** (`openfanctl`): Command-line tool for managing fans

```text
┌─────────────┐     HTTP      ┌──────────┐    Serial   ┌───────────────┐
│  openfanctl │ ────────────> │ openfand │ ──────────> │ OpenFAN Board │
│    (CLI)    │   REST API    │ (Server) │   USB/TTY   │  (Hardware)   │
└─────────────┘               └──────────┘             └───────────────┘
```

## Supported Hardware

| Board | Fans | USB VID:PID | Status |
|-------|------|-------------|--------|
| OpenFAN Standard | 10 | 2E8A:000A | Supported |
| OpenFAN Micro | 1 | 2E8A:000B | Planned |

The server auto-detects which board is connected via USB. In mock mode, you
must specify the board type explicitly.

## Quick Start

### Install from Release

**Debian/Ubuntu:**

```bash
curl -LO https://github.com/graelo/openfan-rs/releases/latest/download/openfan-controller_<VERSION>_amd64.deb
sudo dpkg -i openfan-controller_<VERSION>_amd64.deb
```

**Other Linux:**

```bash
curl -LO https://github.com/graelo/openfan-rs/releases/latest/download/openfan-linux-x86_64.tar.gz
tar xzf openfan-linux-x86_64.tar.gz
sudo ./openfan-linux-x86_64/deploy/install.sh
```

### Build from Source

```bash
git clone https://github.com/graelo/openfan-rs.git
cd openfan-rs
cargo build --release

# Binaries: target/release/openfand, target/release/openfanctl
```

### Run the Server

```bash
# With real hardware (auto-detects board)
openfand

# In mock mode (for testing without hardware)
openfand --mock --board standard   # Simulate OpenFAN Standard (10 fans)
openfand --mock --board micro      # Simulate OpenFAN Micro (1 fan)

# With custom config
openfand --config /path/to/config.toml

# Verbose logging
openfand --verbose
```

### Use the CLI

```bash
# System info (shows board type, fan count)
openfanctl info

# Fan status (RPM readings)
openfanctl status

# Set fan PWM (0-100%)
openfanctl fan set 0 --pwm 75

# Set fan RPM target (500-9000)
openfanctl fan set 0 --rpm 1200

# Apply a profile
openfanctl profile apply "Quiet"

# JSON output
openfanctl --format json status
```

## Configuration

OpenFAN discovers configuration using XDG paths with system fallback:

| Type | User config | System config (fallback) |
|------|--------------|----------------|
| Server config | `~/.config/openfan/config.toml` | `/etc/openfan/config.toml` |
| CLI config | `~/.config/openfan/cli.toml` | — |
| Data (aliases, profiles, zones, curves) | `~/.local/share/openfan/` | `/var/lib/openfan/` |

Both `openfand` and `openfanctl` accept `--config` to override the default path.

```toml
# Directory for mutable data files (profiles, aliases, zones, thermal curves, CFM mappings)
data_dir = "/var/lib/openfan"

[server]
bind_address = "127.0.0.1"
port = 3000
communication_timeout = 1
```

Hardware detection is automatic via USB VID/PID. No hardware configuration needed.

Data files (aliases, profiles, zones, thermal curves) are managed via CLI
commands rather than edited directly. See the [Tutorial](docs/TUTORIAL.md) for
details.

## CLI Commands

```bash
openfanctl info                          # Show board and server info
openfanctl status                        # Show all fans with RPM
openfanctl fan set <id> --pwm <0-100>      # Set fan PWM percentage
openfanctl fan set <id> --rpm <500-9000>  # Set fan RPM target
openfanctl profile list                  # List available profiles
openfanctl profile apply <name>          # Apply a profile
openfanctl alias set <id> <name>         # Set fan alias
openfanctl alias list                    # List all aliases
openfanctl completion <shell>            # Generate shell completion
```

Options:

- `--server <url>` - Server URL (default: <http://localhost:3000>)
- `--format <table|json>` - Output format (default: table)

## REST API

The server exposes a REST API on port 3000 (configurable):

```bash
# System info
curl http://localhost:3000/api/v0/info

# Fan status (all fans)
curl http://localhost:3000/api/v0/fan/status

# Set fan PWM (0-100%)
curl "http://localhost:3000/api/v0/fan/0/pwm?value=75"

# Set fan RPM
curl "http://localhost:3000/api/v0/fan/0/rpm?value=1200"

# List and apply profiles
curl http://localhost:3000/api/v0/profiles/list
curl "http://localhost:3000/api/v0/profiles/set?name=50%25%20PWM"

# Aliases
curl http://localhost:3000/api/v0/alias/all/get
curl "http://localhost:3000/api/v0/alias/0/set?value=CPU%20Fan"
```

See the [Tutorial](docs/TUTORIAL.md) for the complete API reference.

## Docker

```bash
# Mock mode (for testing)
docker run -p 3000:3000 graelo/openfan:latest openfand --mock --board standard

# With real hardware
docker run -p 3000:3000 \
  --device=/dev/ttyUSB0 \
  -v /etc/openfan:/etc/openfan:ro \
  graelo/openfan:latest
```

## Systemd Service

After installation, the service is available:

```bash
sudo systemctl start openfand
sudo systemctl enable openfand
sudo systemctl status openfand

# View logs
sudo journalctl -u openfand -f
```

## Troubleshooting

**Permission denied on /dev/ttyUSB0:**

```bash
sudo usermod -a -G dialout $USER
# Log out and back in
```

**Server won't start:**

```bash
# Check logs
sudo journalctl -u openfand -f

# Test with mock mode
openfand --mock --board standard --verbose
```

**Config/board mismatch error:**
Your config file has profiles with the wrong number of fan values. Either:

- Update the profile values to match your board's fan count
- Delete the config file to regenerate defaults

**CLI can't connect:**

```bash
# Check server is running
curl http://localhost:3000/api/v0/info

# Specify server URL explicitly
openfanctl --server http://localhost:3000 info
```

## Project Structure

```text
openfan-rs/
├── openfan-core/      # Shared types, models, error handling
├── openfan-hardware/  # Serial communication, hardware protocol
├── openfand/          # REST API server (Axum)
└── openfanctl/        # CLI client (clap + reqwest)
```

## License

MIT
