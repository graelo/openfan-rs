# OpenFAN Controller Deployment Guide

This guide covers various deployment methods for the OpenFAN Controller system, from simple binary installation to containerized deployments.

## Table of Contents

- [Prerequisites](#prerequisites)
- [Quick Start](#quick-start)
- [Installation Methods](#installation-methods)
  - [Binary Installation](#binary-installation)
  - [Package Installation](#package-installation)
  - [Container Deployment](#container-deployment)
  - [Manual Installation](#manual-installation)
- [Configuration](#configuration)
- [Hardware Setup](#hardware-setup)
- [Production Deployment](#production-deployment)
- [Monitoring](#monitoring)
- [Troubleshooting](#troubleshooting)

## Prerequisites

### System Requirements

- **Operating System**: Linux (Ubuntu 20.04+, Debian 11+, CentOS 8+, or similar)
- **Architecture**: x86_64 or aarch64
- **Memory**: Minimum 128MB RAM
- **Storage**: 50MB for binaries + configuration
- **Network**: HTTP/HTTPS access for API (default port 8080)

### Hardware Requirements

- **Serial Interface**: USB-to-serial adapter or built-in serial port
- **Fan Hardware**: Compatible OpenFAN controller device
- **Permissions**: Access to `/dev/ttyUSB*` or `/dev/ttyACM*` devices

### Dependencies

- `libc6` (>= 2.17)
- `libgcc-s1` (>= 3.0)
- `systemd` (for service management)

## Quick Start

### 1. Download Release

```bash
# Download latest release
wget https://github.com/graelo/OpenFanController/releases/latest/download/openfan-linux-x86_64.tar.gz

# Extract
tar -xzf openfan-linux-x86_64.tar.gz
cd openfan-linux-x86_64
```

### 2. Install

```bash
# Quick install (requires sudo)
sudo ./deploy/install.sh

# Start service
sudo systemctl enable openfan-server
sudo systemctl start openfan-server
```

### 3. Verify

```bash
# Check service status
sudo systemctl status openfan-server

# Test CLI
openfan info
openfan status
```

## Installation Methods

### Binary Installation

#### Automated Installation Script

The easiest method for most users:

```bash
# Download and run installer
curl -sSL https://github.com/graelo/OpenFanController/raw/main/Software/openfan/deploy/install.sh | sudo bash
```

Or download and inspect first:

```bash
# Download installer
wget https://github.com/graelo/OpenFanController/raw/main/Software/openfan/deploy/install.sh

# Inspect the script
less install.sh

# Run installer
sudo ./install.sh
```

The installer will:
- Create `openfan` user and group
- Install binaries to `/opt/openfan/bin/` and `/usr/local/bin/`
- Set up configuration in `/etc/openfan/`
- Install systemd service
- Configure log rotation
- Install shell completion

#### Manual Binary Installation

For more control over the installation:

```bash
# Download release
VERSION="1.0.0"
ARCH="x86_64"
wget "https://github.com/graelo/OpenFanController/releases/download/v${VERSION}/openfan-linux-${ARCH}.tar.gz"

# Extract
tar -xzf "openfan-linux-${ARCH}.tar.gz"

# Install binaries
sudo mkdir -p /opt/openfan/bin
sudo cp openfan-server /opt/openfan/bin/
sudo cp openfan /usr/local/bin/

# Set permissions
sudo chmod 755 /opt/openfan/bin/openfan-server
sudo chmod 755 /usr/local/bin/openfan

# Create user
sudo useradd --system --shell /bin/false --home-dir /var/lib/openfan --create-home openfan
sudo usermod -a -G dialout openfan
```

### Package Installation

#### Debian/Ubuntu (.deb)

```bash
# Download package
wget https://github.com/graelo/OpenFanController/releases/latest/download/openfan-controller_1.0.0_amd64.deb

# Install
sudo dpkg -i openfan-controller_1.0.0_amd64.deb

# Fix dependencies if needed
sudo apt-get install -f

# Start service
sudo systemctl enable openfan-server
sudo systemctl start openfan-server
```

#### Red Hat/CentOS (.rpm)

*Note: RPM packages are not yet available but can be created using `fpm` or similar tools.*

### Container Deployment

#### Docker

**Basic deployment:**

```bash
# Pull image
docker pull openfan/controller:latest

# Run server
docker run -d \
  --name openfan-server \
  --restart unless-stopped \
  -p 8080:8080 \
  -v openfan-config:/etc/openfan \
  -v openfan-data:/var/lib/openfan \
  openfan/controller:latest
```

**With hardware access:**

```bash
# Run with device access
docker run -d \
  --name openfan-server \
  --restart unless-stopped \
  -p 8080:8080 \
  --device=/dev/ttyUSB0:/dev/ttyUSB0 \
  -v openfan-config:/etc/openfan \
  -v openfan-data:/var/lib/openfan \
  openfan/controller:latest \
  /opt/openfan/bin/openfan-server --config /etc/openfan/config.yaml
```

#### Docker Compose

**Simple deployment:**

```bash
# Clone repository
git clone https://github.com/graelo/OpenFanController.git
cd OpenFanController/Software/openfan

# Start services
docker-compose up -d

# View logs
docker-compose logs -f openfan-server
```

**Mock mode for testing:**

```bash
# Start in mock mode
docker-compose --profile mock up -d openfan-server-mock

# Access at http://localhost:8081
```

**Full monitoring stack:**

```bash
# Start with monitoring
docker-compose --profile monitoring up -d

# Access:
# - OpenFAN API: http://localhost:8080
# - Grafana: http://localhost:3000 (admin/admin)
# - Prometheus: http://localhost:9090
```

#### Kubernetes

Create deployment manifest:

```yaml
apiVersion: apps/v1
kind: Deployment
metadata:
  name: openfan-server
  namespace: openfan
spec:
  replicas: 1
  selector:
    matchLabels:
      app: openfan-server
  template:
    metadata:
      labels:
        app: openfan-server
    spec:
      containers:
      - name: openfan-server
        image: openfan/controller:latest
        ports:
        - containerPort: 8080
        env:
        - name: RUST_LOG
          value: "info"
        volumeMounts:
        - name: config
          mountPath: /etc/openfan
        - name: data
          mountPath: /var/lib/openfan
        livenessProbe:
          httpGet:
            path: /api/v0/info
            port: 8080
          initialDelaySeconds: 30
          periodSeconds: 30
      volumes:
      - name: config
        configMap:
          name: openfan-config
      - name: data
        persistentVolumeClaim:
          claimName: openfan-data
---
apiVersion: v1
kind: Service
metadata:
  name: openfan-service
  namespace: openfan
spec:
  selector:
    app: openfan-server
  ports:
  - port: 8080
    targetPort: 8080
  type: ClusterIP
```

Deploy:

```bash
kubectl create namespace openfan
kubectl apply -f openfan-deployment.yaml
```

### Manual Installation

For custom setups or when automated methods don't work:

#### 1. Build from Source

```bash
# Install Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source ~/.cargo/env

# Clone repository
git clone https://github.com/graelo/OpenFanController.git
cd OpenFanController/Software/openfan

# Build release
cargo build --release

# Install
sudo cp target/release/openfan-server /opt/openfan/bin/
sudo cp target/release/openfan /usr/local/bin/
```

#### 2. Create System User

```bash
sudo groupadd --system openfan
sudo useradd --system --gid openfan --shell /bin/false \
    --home-dir /var/lib/openfan --create-home \
    --comment "OpenFAN Controller" openfan
sudo usermod -a -G dialout openfan
```

#### 3. Create Directories

```bash
sudo mkdir -p /opt/openfan/bin
sudo mkdir -p /etc/openfan
sudo mkdir -p /var/lib/openfan
sudo mkdir -p /var/log/openfan

sudo chown -R openfan:openfan /opt/openfan
sudo chown -R openfan:openfan /etc/openfan
sudo chown -R openfan:openfan /var/lib/openfan
sudo chown -R openfan:openfan /var/log/openfan
```

#### 4. Install Configuration

```bash
sudo cp config.yaml /etc/openfan/
sudo chown openfan:openfan /etc/openfan/config.yaml
sudo chmod 640 /etc/openfan/config.yaml
```

#### 5. Install Systemd Service

```bash
sudo cp deploy/openfan-server.service /etc/systemd/system/
sudo systemctl daemon-reload
sudo systemctl enable openfan-server
```

## Configuration

### Basic Configuration

Edit `/etc/openfan/config.yaml`:

```yaml
server:
  port: 8080
  bind: "127.0.0.1"  # Change to "0.0.0.0" for external access

hardware:
  device_path: "/dev/ttyUSB0"  # Adjust to your device
  baud_rate: 115200
  timeout_ms: 2000

fans:
  count: 10

fan_profiles:
  "Quiet":
    control_mode: "Pwm"
    values: [30, 30, 30, 30, 30, 30, 30, 30, 30, 30]
  "Balanced":
    control_mode: "Pwm"
    values: [50, 50, 50, 50, 50, 50, 50, 50, 50, 50]
  "Performance":
    control_mode: "Pwm"
    values: [80, 80, 80, 80, 80, 80, 80, 80, 80, 80]

fan_aliases:
  0: "CPU Fan"
  1: "GPU Fan"
  2: "Case Fan 1"
  3: "Case Fan 2"
  4: "Radiator Fan 1"
  5: "Radiator Fan 2"
  6: "Intake Fan 1"
  7: "Intake Fan 2"
  8: "Exhaust Fan 1"
  9: "Exhaust Fan 2"
```

### Advanced Configuration

#### Security Settings

For external access, consider using a reverse proxy:

```nginx
# /etc/nginx/sites-available/openfan
server {
    listen 80;
    server_name openfan.example.com;

    location / {
        proxy_pass http://127.0.0.1:8080;
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto $scheme;
    }
}
```

#### Firewall Configuration

```bash
# UFW
sudo ufw allow 8080/tcp
sudo ufw enable

# iptables
sudo iptables -A INPUT -p tcp --dport 8080 -j ACCEPT
sudo iptables-save > /etc/iptables/rules.v4
```

#### SSL/TLS

For HTTPS, use a reverse proxy with SSL termination or configure the server with TLS certificates.

## Hardware Setup

### Device Detection

Check for connected devices:

```bash
# List USB devices
lsusb

# List serial devices
ls -la /dev/tty{USB,ACM}*

# Check device permissions
ls -la /dev/ttyUSB0
```

### Permissions

Ensure the `openfan` user can access the device:

```bash
# Add user to dialout group
sudo usermod -a -G dialout openfan

# Check group membership
groups openfan

# Set device permissions (if needed)
sudo chmod 666 /dev/ttyUSB0
```

### Device Rules

Create udev rules for consistent device naming:

```bash
# /etc/udev/rules.d/99-openfan.rules
SUBSYSTEM=="tty", ATTRS{idVendor}=="2e8a", ATTRS{idProduct}=="000a", GROUP="dialout", MODE="0666", SYMLINK+="openfan"

# Reload rules
sudo udevadm control --reload-rules
sudo udevadm trigger
```

## Production Deployment

### High Availability

#### Load Balancer Configuration

```yaml
# HAProxy configuration
backend openfan_servers
    balance roundrobin
    server openfan1 192.168.1.10:8080 check
    server openfan2 192.168.1.11:8080 check
    server openfan3 192.168.1.12:8080 check
```

#### Database Clustering

*Note: OpenFAN currently uses local YAML files. For HA, consider shared storage or implement database backend.*

### Security Hardening

#### Service Isolation

```ini
# Enhanced systemd service
[Unit]
Description=OpenFAN Controller Server
After=network.target

[Service]
Type=simple
User=openfan
Group=openfan
ExecStart=/opt/openfan/bin/openfan-server --config /etc/openfan/config.yaml
Restart=always
RestartSec=5

# Security settings
NoNewPrivileges=true
PrivateTmp=true
ProtectSystem=strict
ProtectHome=true
ReadWritePaths=/var/lib/openfan /var/log/openfan /etc/openfan
ProtectKernelTunables=true
ProtectKernelModules=true
ProtectControlGroups=true
RestrictRealtime=true
RestrictSUIDSGID=true
LockPersonality=true
MemoryDenyWriteExecute=true
RestrictNamespaces=true
SystemCallFilter=@system-service
SystemCallErrorNumber=EPERM

# Resource limits
LimitNOFILE=65536
LimitNPROC=4096

# Device access
DeviceAllow=/dev/ttyUSB* rw
DeviceAllow=/dev/ttyACM* rw
DevicePolicy=closed

[Install]
WantedBy=multi-user.target
```

#### Network Security

```bash
# Restrict access to specific IPs
iptables -A INPUT -p tcp --dport 8080 -s 192.168.1.0/24 -j ACCEPT
iptables -A INPUT -p tcp --dport 8080 -j DROP
```

### Backup and Recovery

#### Configuration Backup

```bash
#!/bin/bash
# backup-openfan.sh

BACKUP_DIR="/backup/openfan"
DATE=$(date +%Y%m%d_%H%M%S)

mkdir -p "$BACKUP_DIR"

# Backup configuration
tar -czf "$BACKUP_DIR/openfan-config-$DATE.tar.gz" /etc/openfan/

# Backup data
tar -czf "$BACKUP_DIR/openfan-data-$DATE.tar.gz" /var/lib/openfan/

# Keep only last 30 days
find "$BACKUP_DIR" -name "*.tar.gz" -mtime +30 -delete

echo "Backup completed: $DATE"
```

#### Automated Backups

```bash
# Add to crontab
echo "0 2 * * * /usr/local/bin/backup-openfan.sh" | sudo crontab -
```

## Monitoring

### Service Monitoring

#### Systemd Status

```bash
# Check service status
sudo systemctl status openfan-server

# View logs
sudo journalctl -u openfan-server -f

# Check resource usage
sudo systemctl show openfan-server --property=MainPID,MemoryCurrent,CPUUsageNSec
```

#### Health Checks

```bash
# CLI health check
openfan health

# API health check
curl -s http://localhost:8080/api/v0/info | jq '.software.version'

# Advanced monitoring script
#!/bin/bash
# health-check.sh

SERVER_URL="http://localhost:8080"
CLI_PATH="/usr/local/bin/openfan"

# Check API availability
if curl -sf "$SERVER_URL/api/v0/info" >/dev/null; then
    echo "✓ API server is responding"
else
    echo "✗ API server is not responding"
    exit 1
fi

# Check CLI connectivity
if "$CLI_PATH" --server "$SERVER_URL" info >/dev/null 2>&1; then
    echo "✓ CLI can connect to server"
else
    echo "✗ CLI cannot connect to server"
    exit 1
fi

# Check hardware status
if "$CLI_PATH" --server "$SERVER_URL" status >/dev/null 2>&1; then
    echo "✓ Hardware is accessible"
else
    echo "⚠ Hardware may not be available (check mock mode)"
fi

echo "Health check completed"
```

### Log Management

#### Log Rotation

```bash
# /etc/logrotate.d/openfan
/var/log/openfan/*.log {
    daily
    rotate 30
    compress
    delaycompress
    missingok
    notifempty
    create 640 openfan openfan
    postrotate
        systemctl reload-or-restart openfan-server
    endscript
}
```

#### Centralized Logging

```yaml
# Fluentd configuration
<source>
  @type tail
  path /var/log/openfan/*.log
  pos_file /var/log/fluentd/openfan.log.pos
  tag openfan.*
  format json
</source>

<match openfan.**>
  @type elasticsearch
  host elasticsearch.example.com
  port 9200
  index_name openfan
</match>
```

### Metrics Collection

#### Prometheus Metrics

*Note: Metrics endpoint not yet implemented in OpenFAN. Consider adding custom metrics exporter.*

```bash
# Example metrics script
#!/bin/bash
# openfan-metrics.sh

SERVER_URL="http://localhost:8080"
METRICS_FILE="/var/lib/node_exporter/textfile_collector/openfan.prom"

# Get system info
INFO=$(curl -s "$SERVER_URL/api/v0/info" | jq -r '.software.version // "unknown"')

# Get fan status
STATUS=$(curl -s "$SERVER_URL/api/v0/fan/status")

# Export metrics
cat > "$METRICS_FILE" << EOF
# HELP openfan_version OpenFAN version info
# TYPE openfan_version gauge
openfan_version{version="$INFO"} 1

# HELP openfan_fan_rpm Current fan RPM
# TYPE openfan_fan_rpm gauge
EOF

# Add fan RPM metrics
echo "$STATUS" | jq -r '.rpms | to_entries[] | "openfan_fan_rpm{fan_id=\"\(.key)\"} \(.value)"' >> "$METRICS_FILE"

echo "Metrics updated"
```

## Troubleshooting

### Common Issues

#### Service Won't Start

```bash
# Check service status
sudo systemctl status openfan-server

# View detailed logs
sudo journalctl -u openfan-server --no-pager

# Check configuration
openfan-server --config /etc/openfan/config.yaml --help

# Test configuration
sudo -u openfan /opt/openfan/bin/openfan-server --config /etc/openfan/config.yaml --mock
```

#### Permission Denied

```bash
# Check user permissions
sudo -u openfan ls -la /dev/ttyUSB0

# Fix permissions
sudo usermod -a -G dialout openfan
sudo chmod 666 /dev/ttyUSB0

# Restart service
sudo systemctl restart openfan-server
```

#### Hardware Not Found

```bash
# List available devices
ls -la /dev/tty{USB,ACM}*

# Check USB devices
lsusb | grep -i "2e8a:000a"

# Test device communication
sudo screen /dev/ttyUSB0 115200

# Run in mock mode
sudo systemctl edit openfan-server
# Add:
# [Service]
# ExecStart=
# ExecStart=/opt/openfan/bin/openfan-server --config /etc/openfan/config.yaml --mock
```

#### CLI Connection Issues

```bash
# Test server connectivity
curl -s http://localhost:8080/api/v0/info

# Check firewall
sudo ufw status
sudo iptables -L INPUT -n

# Test CLI with different server URL
openfan --server http://localhost:8080 info

# Enable debug logging
RUST_LOG=debug openfan info
```

### Log Analysis

#### Common Log Patterns

```bash
# Show startup errors
sudo journalctl -u openfan-server | grep -i error

# Show hardware connection attempts
sudo journalctl -u openfan-server | grep -i "hardware"

# Show API requests
sudo journalctl -u openfan-server | grep -i "request"

# Show configuration loading
sudo journalctl -u openfan-server | grep -i "config"
```

#### Performance Issues

```bash
# Check resource usage
top -p $(pgrep openfan-server)
sudo systemctl show openfan-server --property=MemoryCurrent,CPUUsageNSec

# Check disk space
df -h /var/lib/openfan /var/log/openfan

# Monitor network connections
sudo netstat -tulpn | grep 8080
sudo ss -tulpn | grep 8080
```

### Getting Help

1. **Check logs**: Always start with `sudo journalctl -u openfan-server`
2. **Test configuration**: Use `--mock` mode to isolate hardware issues
3. **Verify permissions**: Ensure `openfan` user has device access
4. **Check network**: Verify firewall and port availability
5. **Update software**: Ensure you're running the latest version

#### Support Resources

- **GitHub Issues**: https://github.com/graelo/OpenFanController/issues
- **Documentation**: https://github.com/graelo/OpenFanController/blob/main/Software/openfan/README.md
- **Discord/Forum**: [Community link if available]

#### Reporting Issues

Include the following information:

```bash
# System information
uname -a
cat /etc/os-release

# OpenFAN version
openfan --version || /opt/openfan/bin/openfan-server --version

# Service status
sudo systemctl status openfan-server

# Recent logs
sudo journalctl -u openfan-server --since "1 hour ago"

# Hardware information
lsusb
ls -la /dev/tty{USB,ACM}*

# Configuration (remove sensitive data)
sudo cat /etc/openfan/config.yaml
```

---

This deployment guide covers the most common deployment scenarios. For specific environments or advanced configurations, consult the project documentation or community support channels.