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
openfand --mock --board standard

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
openfand --mock --board standard
openfand --mock --board micro
```

### Configuration Files

OpenFAN uses XDG-compliant paths by default:

| File | Location | Purpose |
|------|----------|---------|
| Static config | `~/.config/openfan/config.toml` | Server settings |
| Aliases | `~/.local/share/openfan/aliases.toml` | Fan names |
| Profiles | `~/.local/share/openfan/profiles.toml` | Saved profiles |
| Zones | `~/.local/share/openfan/zones.toml` | Fan groups |
| Thermal curves | `~/.local/share/openfan/thermal_curves.toml` | Temperature-PWM curves |
| CFM mappings | `~/.local/share/openfan/cfm_mappings.toml` | Airflow display values |

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
# Set fan 0 to 50% PWM (valid range: 0-100)
openfanctl fan set 0 --pwm 50

# Set fan 3 to 1200 RPM (valid range: 500-9000)
openfanctl fan set 3 --rpm 1200

# Get current RPM
openfanctl fan rpm 0

# Get current PWM
openfanctl fan pwm 0
```

### Fan IDs

Fan IDs are 0-indexed:

- OpenFAN Standard: fans 0-9 (10 fans)
- OpenFAN Micro: fan 0 (1 fan)

### Control Modes

OpenFAN supports two control modes:

- **PWM mode**: Set fan speed as percentage (0-100%)
- **RPM target mode**: Set target RPM (500-9000) - hardware adjusts PWM to reach
  target

## Profiles

Profiles store preset fan configurations that can be applied instantly.

### Managing Profiles

```bash
# List all profiles
openfanctl profile list

# Apply a profile
openfanctl profile apply "50% PWM"

# Add a new PWM profile (10 values for OpenFAN Standard)
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

### Profile Value Ranges

- **PWM profiles**: values must be 0-100 (percentage)
- **RPM profiles**: values must be 500-9000 (target RPM)

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

Zones group multiple fans for coordinated control. Each fan port can belong to
at most one zone.

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

## Thermal Curves

Thermal curves define temperature-to-PWM mappings for automatic fan speed
control based on temperature readings. Each curve consists of points that map
temperatures to PWM values, with linear interpolation between points.

### Default Curves

The server creates these default thermal curves:

| Name | Description | Points |
|------|-------------|--------|
| Balanced | Standard curve for balanced performance | 30°C→25%, 50°C→50%, 70°C→80%, 85°C→100% |
| Silent | Low noise curve for quiet operation | 40°C→20%, 60°C→40%, 80°C→70%, 90°C→100% |
| Aggressive | High cooling for maximum performance | 30°C→40%, 50°C→70%, 65°C→90%, 75°C→100% |

### Managing Thermal Curves

```bash
# List all curves
openfanctl curve list

# Get details for a specific curve
openfanctl curve get Balanced

# Add a new curve (format: "temp:pwm,temp:pwm,...")
openfanctl curve add Custom --points "25:20,45:40,65:70,80:100" --description "Custom curve"

# Update an existing curve
openfanctl curve update Custom --points "30:25,50:50,70:80,90:100"

# Delete a curve
openfanctl curve delete Custom
```

### Interpolating Values

You can query what PWM value a curve would produce for any temperature:

```bash
# Get PWM for 55°C using the Balanced curve
openfanctl curve interpolate Balanced --temp 55.0
# Output: Curve 'Balanced' at 55°C = 62% PWM

# JSON output
openfanctl curve interpolate Balanced --temp 55.0 --format json
# Output: {"temperature":55.0,"pwm":62}
```

### How Interpolation Works

The curve uses linear interpolation between defined points:

- **Below minimum**: Returns the lowest point's PWM value
- **Above maximum**: Returns the highest point's PWM value
- **Between points**: Linearly interpolates based on position

For example, with points `30:25` and `50:50`:

- At 30°C → 25% PWM
- At 40°C → 37% PWM (midpoint)
- At 50°C → 50% PWM

### Points Format

CLI uses colon-separated pairs: `"temp:pwm,temp:pwm,..."`

Requirements:

- At least 2 points required
- Points are automatically sorted by temperature
- PWM values must be 0-100
- Temperature range: -50°C to 150°C

Examples:

```bash
# Simple 2-point curve
--points "30:30,80:100"

# Detailed 5-point curve
--points "25:20,40:35,55:50,70:75,85:100"
```

## CFM Mappings

CFM (Cubic Feet per Minute) mappings allow you to display estimated airflow
values in the status output. This is a display-only feature - it doesn't affect
fan control.

Each mapping stores the CFM value at 100% PWM for a specific fan port. The
actual CFM is calculated using linear interpolation:
`cfm = (pwm / 100) * cfm_at_100`.

### Managing CFM Mappings

```bash
# List all CFM mappings
openfanctl cfm list

# Get CFM mapping for port 0
openfanctl cfm get 0

# Set CFM@100% for port 0 (e.g., a fan rated at 45 CFM)
openfanctl cfm set 0 --cfm-at-100 45.0

# Set CFM for multiple ports
openfanctl cfm set 1 --cfm-at-100 45.0
openfanctl cfm set 2 --cfm-at-100 60.0

# Delete a CFM mapping
openfanctl cfm delete 0
```

### Status Output with CFM

When CFM mappings are configured, the status command shows an additional CFM
column:

```bash
$ openfanctl status
Fan Status:
╭────────┬──────┬───────┬───────╮
│ Fan ID │ RPM  │ PWM % │ CFM   │
├────────┼──────┼───────┼───────┤
│ 0      │ 1200 │ 75%   │ 33.8  │
│ 1      │ 800  │ 50%   │ 22.5  │
│ 2      │ 950  │ 60%   │ -     │
╰────────┴──────┴───────┴───────╯
```

- CFM values are shown with 1 decimal place
- Ports without mappings show `-`
- The CFM column only appears when at least one mapping exists

### JSON Output

```bash
$ openfanctl status --format json
{
  "rpms": {"0": 1200, "1": 800, "2": 950},
  "pwms": {"0": 75, "1": 50, "2": 60},
  "cfm": {"0": 33.75, "1": 22.5}
}
```

### Finding Your Fan's CFM Rating

Most fans list their CFM rating in the specifications. Common values:

- 120mm case fans: 30-70 CFM
- 140mm case fans: 50-100 CFM
- 80mm fans: 15-35 CFM

Use the CFM@100% value from your fan's specifications.

### Validation

- CFM values must be positive (> 0)
- Maximum allowed value is 500 CFM
- Port IDs must be valid for your board (0-9 for OpenFAN Standard, 0 for Micro)

## REST API

The server exposes a REST API on port 3000 (default).

### Endpoints

| Endpoint | Method | Description |
|----------|--------|-------------|
| `/api/v0/info` | GET | Server info |
| `/api/v0/fan/status` | GET | All fan status |
| `/api/v0/fan/:id/pwm?value=N` | GET | Set fan PWM (0-100) |
| `/api/v0/fan/:id/rpm?value=N` | GET | Set fan RPM target (500-9000) |
| `/api/v0/profiles/list` | GET | List profiles |
| `/api/v0/profiles/set?name=X` | GET | Apply profile |
| `/api/v0/profiles/add` | POST | Add profile |
| `/api/v0/alias/all/get` | GET | Get all aliases |
| `/api/v0/alias/:id/set?value=X` | GET | Set alias |
| `/api/v0/alias/:id` | DELETE | Delete alias (revert to default) |
| `/api/v0/zones/list` | GET | List zones |
| `/api/v0/zones/add` | POST | Add zone |
| `/api/v0/zone/:name/apply?mode=pwm&value=N` | GET | Apply to zone |
| `/api/v0/curves/list` | GET | List thermal curves |
| `/api/v0/curves/add` | POST | Add thermal curve |
| `/api/v0/curve/:name/get` | GET | Get curve details |
| `/api/v0/curve/:name/update` | POST | Update curve |
| `/api/v0/curve/:name` | DELETE | Delete curve |
| `/api/v0/curve/:name/interpolate?temp=N` | GET | Interpolate PWM for temperature |
| `/api/v0/cfm/list` | GET | List CFM mappings |
| `/api/v0/cfm/:port` | GET | Get CFM mapping for port |
| `/api/v0/cfm/:port` | POST | Set CFM mapping `{"cfm_at_100": 45.0}` |
| `/api/v0/cfm/:port` | DELETE | Delete CFM mapping |

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

# List thermal curves
curl http://localhost:3000/api/v0/curves/list

# Add a thermal curve (POST with JSON body)
curl -X POST http://localhost:3000/api/v0/curves/add \
  -H "Content-Type: application/json" \
  -d '{"name":"Custom","points":[{"temp_c":30,"pwm":25},{"temp_c":70,"pwm":100}]}'

# Interpolate temperature
curl http://localhost:3000/api/v0/curve/Balanced/interpolate?temp=55

# List CFM mappings
curl http://localhost:3000/api/v0/cfm/list

# Get CFM mapping for port 0
curl http://localhost:3000/api/v0/cfm/0

# Set CFM mapping (POST with JSON body)
curl -X POST http://localhost:3000/api/v0/cfm/0 \
  -H "Content-Type: application/json" \
  -d '{"cfm_at_100": 45.0}'

# Delete CFM mapping
curl -X DELETE http://localhost:3000/api/v0/cfm/0
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
openfand --mock --board standard
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

- OpenFAN Standard: 10 values
- OpenFAN Micro: 1 value

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

# Set CFM ratings for airflow monitoring (from fan specs)
openfanctl cfm set 0 --cfm-at-100 52.0   # Noctua NF-A12x25
openfanctl cfm set 1 --cfm-at-100 52.0
openfanctl cfm set 2 --cfm-at-100 63.0   # Arctic P12
openfanctl cfm set 3 --cfm-at-100 63.0
openfanctl cfm set 4 --cfm-at-100 63.0
openfanctl cfm set 5 --cfm-at-100 56.0   # Be Quiet Silent Wings 3
openfanctl cfm set 6 --cfm-at-100 56.0
openfanctl cfm set 7 --cfm-at-100 56.0
openfanctl cfm set 8 --cfm-at-100 56.0
openfanctl cfm set 9 --cfm-at-100 56.0

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
