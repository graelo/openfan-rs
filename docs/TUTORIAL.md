# OpenFAN Tutorial

A complete guide to setting up and using the OpenFAN fan controller system.

## Overview

OpenFAN consists of two components:

- **openfand** - The daemon/server that communicates with hardware
- **openfanctl** - The CLI client for controlling fans

## Quick Start

```bash
# Start the server (with hardware connected)
openfand

# Or in mock mode for testing
openfand --mock openfan-v1

# Check status
openfanctl status
```

## Server Setup

### Starting the Server

```bash
# Auto-detect hardware
openfand

# Specify config file
openfand --config /etc/openfan/config.toml

# Mock mode (no hardware required)
openfand --mock openfan-v1
openfand --mock openfan-mini
```

### Configuration Files

OpenFAN uses XDG-compliant paths by default:

| File | Location | Purpose |
|------|----------|---------|
| Static config | `~/.config/openfan/config.toml` | Server settings |
| Aliases | `~/.local/share/openfan/aliases.toml` | Fan names |
| Profiles | `~/.local/share/openfan/profiles.toml` | Saved profiles |
| Zones | `~/.local/share/openfan/zones.toml` | Fan groups |

For system-wide installations, use `/etc/openfan/` and `/var/lib/openfan/`.

#### Example config.toml

```toml
[server]
bind_address = "127.0.0.1"
port = 3000

[hardware]
connection_type = "auto"

# Custom data directory (optional)
data_dir = "/var/lib/openfan"
```

## CLI Usage

### Basic Commands

```bash
# Server info
openfanctl info

# Fan status (PWM and RPM for all fans)
openfanctl status

# Health check
openfanctl health
```

### Output Formats

```bash
# Table format (default)
openfanctl status

# JSON format
openfanctl status --format json

# Set default format
openfanctl config set output_format json
```

### CLI Configuration

The CLI has its own configuration at `~/.config/openfan/cli.toml`:

```bash
# View current config
openfanctl config show

# Set server URL
openfanctl config set server_url http://192.168.1.100:3000

# Reset to defaults
openfanctl config reset
```

## Fan Control

### Individual Fan Control

```bash
# Set fan 0 to 50% PWM
openfanctl fan set 0 --pwm 50

# Set fan 3 to 1200 RPM
openfanctl fan set 3 --rpm 1200

# Get current RPM
openfanctl fan rpm 0

# Get current PWM
openfanctl fan pwm 0
```

### Fan IDs

Fan IDs are 0-indexed:
- OpenFAN v1.0: fans 0-9 (10 fans)
- OpenFAN Mini: fans 0-3 (4 fans)

## Profiles

Profiles store preset fan configurations that can be applied instantly.

### Managing Profiles

```bash
# List all profiles
openfanctl profile list

# Apply a profile
openfanctl profile apply "50% PWM"

# Add a new PWM profile (10 values for OpenFAN v1.0)
openfanctl profile add "Silent" pwm 30,30,30,30,30,30,30,30,30,30

# Add an RPM profile
openfanctl profile add "Gaming" rpm 1500,1500,2000,2000,1500,1500,1500,1500,1500,1500

# Remove a profile
openfanctl profile remove "Silent"
```

### Default Profiles

The server creates these default profiles:
- `50% PWM` - All fans at 50%
- `100% PWM` - All fans at 100%
- `1000 RPM` - All fans at 1000 RPM

## Aliases

Aliases give human-readable names to fan ports.

```bash
# List all aliases
openfanctl alias list

# Get alias for fan 0
openfanctl alias get 0

# Set alias
openfanctl alias set 0 "CPU Intake"
openfanctl alias set 1 "GPU Exhaust"

# Delete alias (reverts to default "Fan #N")
openfanctl alias delete 0
```

Aliases support alphanumeric characters, hyphens, underscores, dots, and spaces.

## Zones

Zones group multiple fans for coordinated control. Each fan port can belong to at most one zone.

### Creating Zones

```bash
# Create a zone with ports 0, 1, 2
openfanctl zone add intake --ports 0,1,2 --description "Front intake fans"

# Create another zone
openfanctl zone add exhaust --ports 3,4 --description "Rear exhaust fans"
```

### Managing Zones

```bash
# List all zones
openfanctl zone list

# Get zone details
openfanctl zone get intake

# Update zone ports
openfanctl zone update intake --ports 0,1,2,5

# Delete a zone
openfanctl zone delete exhaust
```

### Applying Values to Zones

```bash
# Set all fans in zone to 75% PWM
openfanctl zone apply intake --pwm 75

# Set all fans in zone to 1500 RPM
openfanctl zone apply exhaust --rpm 1500
```

## REST API

The server exposes a REST API on port 3000 (default).

### Endpoints

| Endpoint | Method | Description |
|----------|--------|-------------|
| `/api/v0/info` | GET | Server info |
| `/api/v0/fan/status` | GET | All fan status |
| `/api/v0/fan/:id/pwm?value=N` | GET | Set fan PWM |
| `/api/v0/fan/:id/rpm?value=N` | GET | Set fan RPM |
| `/api/v0/profiles/list` | GET | List profiles |
| `/api/v0/profiles/set?name=X` | GET | Apply profile |
| `/api/v0/profiles/add` | POST | Add profile |
| `/api/v0/alias/all/get` | GET | Get all aliases |
| `/api/v0/alias/:id/set?value=X` | GET | Set alias |
| `/api/v0/alias/:id` | DELETE | Delete alias (revert to default) |
| `/api/v0/zones/list` | GET | List zones |
| `/api/v0/zones/add` | POST | Add zone |
| `/api/v0/zone/:name/apply?mode=pwm&value=N` | GET | Apply to zone |

### Example API Calls

```bash
# Get server info
curl http://localhost:3000/api/v0/info

# Set fan 0 to 50% PWM
curl http://localhost:3000/api/v0/fan/0/pwm?value=50

# Apply a profile
curl http://localhost:3000/api/v0/profiles/set?name=50%25%20PWM

# Add a zone (POST with JSON body)
curl -X POST http://localhost:3000/api/v0/zones/add \
  -H "Content-Type: application/json" \
  -d '{"name":"intake","port_ids":[0,1,2],"description":"Front fans"}'

# Apply PWM to zone
curl http://localhost:3000/api/v0/zone/intake/apply?mode=pwm&value=75
```

## Shell Completion

Generate completion scripts for your shell:

```bash
# Bash
openfanctl completion bash > /etc/bash_completion.d/openfanctl

# Zsh
openfanctl completion zsh > ~/.zsh/completions/_openfanctl

# Fish
openfanctl completion fish > ~/.config/fish/completions/openfanctl.fish
```

## Running as a Service

### systemd (Linux)

Create `/etc/systemd/system/openfand.service`:

```ini
[Unit]
Description=OpenFAN Daemon
After=network.target

[Service]
Type=simple
ExecStart=/usr/local/bin/openfand --config /etc/openfan/config.toml
Restart=on-failure

[Install]
WantedBy=multi-user.target
```

Then:

```bash
sudo systemctl daemon-reload
sudo systemctl enable openfand
sudo systemctl start openfand
```

### launchd (macOS)

Create `~/Library/LaunchAgents/com.openfan.daemon.plist`:

```xml
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>com.openfan.daemon</string>
    <key>ProgramArguments</key>
    <array>
        <string>/usr/local/bin/openfand</string>
    </array>
    <key>RunAtLoad</key>
    <true/>
    <key>KeepAlive</key>
    <true/>
</dict>
</plist>
```

Then:

```bash
launchctl load ~/Library/LaunchAgents/com.openfan.daemon.plist
```

## Troubleshooting

### Server won't start

```bash
# Check if port is in use
lsof -i :3000

# Run with verbose logging
RUST_LOG=debug openfand
```

### Hardware not detected

```bash
# Check USB connection
lsusb | grep -i fan

# Try mock mode to verify software
openfand --mock openfan-v1
```

### CLI can't connect

```bash
# Test connectivity
openfanctl health

# Check server URL
openfanctl config show

# Use verbose mode
openfanctl --verbose status
```

### Profile has wrong number of values

Profiles must have exactly as many values as the board has fans:
- OpenFAN v1.0: 10 values
- OpenFAN Mini: 4 values

```bash
# Check board info
openfanctl info
```

## Common Workflows

### Setting up a gaming PC

```bash
# Name your fans
openfanctl alias set 0 "CPU Intake"
openfanctl alias set 1 "CPU Exhaust"
openfanctl alias set 2 "GPU Intake 1"
openfanctl alias set 3 "GPU Intake 2"
openfanctl alias set 4 "GPU Exhaust"
openfanctl alias set 5 "Case Front 1"
openfanctl alias set 6 "Case Front 2"
openfanctl alias set 7 "Case Top 1"
openfanctl alias set 8 "Case Top 2"
openfanctl alias set 9 "Case Rear"

# Create zones
openfanctl zone add cpu --ports 0,1 --description "CPU cooling"
openfanctl zone add gpu --ports 2,3,4 --description "GPU cooling"
openfanctl zone add case --ports 5,6,7,8,9 --description "Case airflow"

# Create profiles
openfanctl profile add "Idle" pwm 30,30,30,30,30,30,30,30,30,30
openfanctl profile add "Gaming" pwm 50,60,70,70,60,50,50,60,60,50
openfanctl profile add "Rendering" pwm 80,80,100,100,100,70,70,80,80,70

# Apply based on activity
openfanctl profile apply "Gaming"

# Or control by zone
openfanctl zone apply gpu --pwm 80
```

### Server cooling with zones

```bash
# Create intake/exhaust zones
openfanctl zone add intake --ports 0,1,2,3,4 --description "Front intake"
openfanctl zone add exhaust --ports 5,6,7,8,9 --description "Rear exhaust"

# Set intake slightly higher for positive pressure
openfanctl zone apply intake --pwm 60
openfanctl zone apply exhaust --pwm 50
```
