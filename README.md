# OpenFAN Controller

Control your fans via a REST API and CLI. Rust-based, fast, and reliable.

## What is this?

OpenFAN connects to your fan controller hardware over serial and lets you manage fans through a simple REST API or command-line tool. Set PWM/RPM values, create profiles, and monitor everything.

## Quick Start

### Installation on Linux

**Install on Debian/Ubuntu:**

```bash
# Download package
curl -LO https://github.com/graelo/openfan-rs/releases/latest/download/openfan-controller_0.1.0_amd64.deb

# Install
sudo dpkg -i openfan-controller_0.1.0_amd64.deb
sudo apt-get install -f  # Fix dependencies if needed
```

**Install on other Linux:**

```bash
# Download release
curl -LO https://github.com/graelo/openfan-rs/releases/latest/download/openfan-linux-x86_64.tar.gz
tar xzf openfan-linux-x86_64.tar.gz
cd openfan-linux-x86_64

# Install (creates systemd service)
sudo ./deploy/install.sh
```

### Basic Usage

**Start the server:**

```bash
# With hardware (auto-detects board type)
sudo systemctl start openfand

# Or in mock mode (requires explicit board type)
openfand --mock --board v1      # Test with OpenFAN v1.0 (10 fans)
openfand --mock --board mini    # Test with OpenFAN Mini (4 fans)
```

**Use the CLI:**

```bash
# Check system info
openfanctl info

# See all fans
openfanctl status

# Set a fan to 75% PWM
openfanctl fan set 0 --pwm 75

# Apply a profile
openfanctl profile apply "Performance"
```

That's it. 🎉

## CLI Commands

```bash
openfanctl status                           # Show all fans
openfanctl fan set <id> --pwm <0-100>      # Set fan PWM %
openfanctl fan set <id> --rpm <value>      # Set fan RPM
openfanctl profile list                     # List profiles
openfanctl profile apply <name>             # Apply profile
openfanctl alias set <id> <name>            # Name a fan
```

Use `--format json` for JSON output.

## Configuration

Edit `/etc/openfan/config.yaml`:

```yaml
server:
  port: 8080
  bind: "127.0.0.1"

hardware:
  device_path: "/dev/ttyUSB0"
  baud_rate: 115200

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
```

## Docker

```bash
# Run with mock hardware (requires board type)
docker run -p 8080:8080 graelo/openfan:latest openfand --mock --board v1

# Run with real hardware (auto-detects board)
docker run -p 8080:8080 \
  --device=/dev/ttyUSB0 \
  graelo/openfan:latest \
  openfand --config /etc/openfan/config.yaml
```

## REST API

The server exposes a REST API on port 8080:

```bash
# Get system info
curl http://localhost:8080/api/v0/info

# Get fan status
curl http://localhost:8080/api/v0/fan/status

# Set fan 0 to 75% PWM
curl -X POST http://localhost:8080/api/v0/fan/0/pwm/75

# List profiles
curl http://localhost:8080/api/v0/profile/list
```

See `/api/v0/` for all endpoints.

## Building from Source

```bash
# Install Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Build
git clone https://github.com/graelo/openfan-rs.git
cd openfan-rs
cargo build --release

# Binaries are in target/release/
./target/release/openfand --mock --board v1
./target/release/openfanctl info
```

## Project Structure

This is a Rust workspace with 4 crates:

- **openfan-core** - Shared types and models
- **openfan-hardware** - Serial communication and hardware protocol
- **openfand** - REST API server (binary)
- **openfanctl** - CLI tool (binary)

## Troubleshooting

**Server won't start:**

```bash
# Check logs
sudo journalctl -u openfand -f

# Test in mock mode (requires board type)
openfand --mock --board v1
```

**Permission denied on /dev/ttyUSB0:**

```bash
# Add user to dialout group
sudo usermod -a -G dialout $USER
# Log out and back in
```

**CLI can't connect:**

```bash
# Check server is running
curl http://localhost:8080/api/v0/info

# Specify server explicitly
openfanctl --server http://localhost:8080 info
```

## License

MIT

## Version

0.1.0
