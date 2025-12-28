# OpenFAN Controller

A Rust-based controller for OpenFAN hardware - manage your fans via REST API or CLI.

## Overview

OpenFAN is a fan controller system consisting of:
- **Hardware**: A microcontroller board that controls PWM fans via USB serial
- **Server** (`openfand`): REST API daemon that communicates with the hardware
- **CLI** (`openfanctl`): Command-line tool for managing fans

```
┌─────────────┐     HTTP      ┌──────────┐    Serial    ┌──────────────┐
│  openfanctl │ ────────────> │ openfand │ ──────────> │ OpenFAN Board │
│    (CLI)    │   REST API    │ (Server) │   USB/TTY   │  (Hardware)   │
└─────────────┘               └──────────┘              └──────────────┘
```

## Supported Hardware

| Board | Fans | USB VID:PID | Status |
|-------|------|-------------|--------|
| OpenFAN v1.0 | 10 | 2E8A:000A | Supported |
| OpenFAN Mini | 4 | 2E8A:000B | Planned |

The server auto-detects which board is connected via USB. In mock mode, you must specify the board type explicitly.

## Quick Start

### Install from Release

**Debian/Ubuntu:**
```bash
curl -LO https://github.com/graelo/openfan-rs/releases/latest/download/openfan-controller_0.1.0_amd64.deb
sudo dpkg -i openfan-controller_0.1.0_amd64.deb
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
openfand --mock --board v1      # Simulate OpenFAN v1.0 (10 fans)
openfand --mock --board mini    # Simulate OpenFAN Mini (4 fans)

# With custom config
openfand --config /path/to/config.yaml

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

# Set fan RPM target
openfanctl fan set 0 --rpm 1200

# Apply a profile
openfanctl profile apply "Quiet"

# JSON output
openfanctl --format json status
```

## Configuration

Config file location: `/etc/openfan/config.yaml` (or specify with `--config`)

```yaml
server:
  port: 3000
  bind: "127.0.0.1"

hardware:
  device_path: "/dev/ttyUSB0"    # Auto-detected if not specified
  baud_rate: 115200

# Profiles must match your board's fan count
# OpenFAN v1.0 = 10 values, OpenFAN Mini = 4 values
fan_profiles:
  "Quiet":
    type: pwm
    values: [30, 30, 30, 30, 30, 30, 30, 30, 30, 30]
  "Performance":
    type: pwm
    values: [80, 80, 80, 80, 80, 80, 80, 80, 80, 80]

fan_aliases:
  0: "CPU Fan"
  1: "GPU Fan"
  2: "Case Front"
```

The server validates your config against the detected board at startup. If your profile has 10 values but you connect an OpenFAN Mini (4 fans), the server will refuse to start with a clear error message.

## CLI Commands

```bash
openfanctl info                          # Show board and server info
openfanctl status                        # Show all fans with RPM
openfanctl fan set <id> --pwm <0-100>   # Set fan PWM percentage
openfanctl fan set <id> --rpm <value>   # Set fan RPM target
openfanctl profile list                  # List available profiles
openfanctl profile apply <name>          # Apply a profile
openfanctl alias set <id> <name>         # Set fan alias
openfanctl alias list                    # List all aliases
openfanctl completion <shell>            # Generate shell completion
```

Options:
- `--server <url>` - Server URL (default: http://localhost:3000)
- `--format <table|json>` - Output format (default: table)

## REST API

The server exposes a REST API on port 3000 (configurable):

```bash
# System info
curl http://localhost:3000/api/v0/info

# Fan status
curl http://localhost:3000/api/v0/fan/status

# Single fan status
curl http://localhost:3000/api/v0/fan/0/status

# Set fan PWM
curl -X POST http://localhost:3000/api/v0/fan/0/pwm/75

# Set fan RPM
curl -X POST http://localhost:3000/api/v0/fan/0/rpm/1200

# List profiles
curl http://localhost:3000/api/v0/profile/list

# Apply profile
curl -X POST http://localhost:3000/api/v0/profile/apply/Quiet

# Get/set aliases
curl http://localhost:3000/api/v0/alias/list
curl -X POST "http://localhost:3000/api/v0/alias/0/CPU%20Fan"
```

## Docker

```bash
# Mock mode (for testing)
docker run -p 3000:3000 graelo/openfan:latest openfand --mock --board v1

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
openfand --mock --board v1 --verbose
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

```
openfan-rs/
├── openfan-core/      # Shared types, models, error handling
├── openfan-hardware/  # Serial communication, hardware protocol
├── openfand/          # REST API server (Axum)
└── openfanctl/        # CLI client (clap + reqwest)
```

## License

MIT

## Version

0.1.0
