#!/bin/bash
# OpenFAN Controller Installation Script
# This script installs the OpenFAN server and CLI tools

set -e  # Exit on any error

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Configuration
INSTALL_DIR="/opt/openfan"
CONFIG_DIR="/etc/openfan"
DATA_DIR="/var/lib/openfan"
LOG_DIR="/var/log/openfan"
USER="openfan"
GROUP="openfan"
SERVICE_NAME="openfan-server"

# Print colored output
print_status() {
    echo -e "${BLUE}[INFO]${NC} $1"
}

print_success() {
    echo -e "${GREEN}[SUCCESS]${NC} $1"
}

print_warning() {
    echo -e "${YELLOW}[WARNING]${NC} $1"
}

print_error() {
    echo -e "${RED}[ERROR]${NC} $1"
}

# Check if running as root
check_root() {
    if [[ $EUID -ne 0 ]]; then
        print_error "This script must be run as root"
        exit 1
    fi
}

# Check system requirements
check_requirements() {
    print_status "Checking system requirements..."

    # Check OS
    if [[ ! -f /etc/os-release ]]; then
        print_error "Cannot determine OS version"
        exit 1
    fi

    # Check systemd
    if ! command -v systemctl &> /dev/null; then
        print_error "systemd is required but not found"
        exit 1
    fi

    # Check if binaries exist
    if [[ ! -f "./target/release/openfan-server" ]]; then
        print_error "openfan-server binary not found. Please run 'cargo build --release' first"
        exit 1
    fi

    if [[ ! -f "./target/release/openfan" ]]; then
        print_error "openfan CLI binary not found. Please run 'cargo build --release' first"
        exit 1
    fi

    print_success "System requirements check passed"
}

# Create user and group
create_user() {
    print_status "Creating user and group..."

    if ! getent group "$GROUP" &>/dev/null; then
        groupadd --system "$GROUP"
        print_success "Created group: $GROUP"
    else
        print_warning "Group $GROUP already exists"
    fi

    if ! getent passwd "$USER" &>/dev/null; then
        useradd --system --gid "$GROUP" --shell /bin/false \
                --home-dir "$DATA_DIR" --create-home \
                --comment "OpenFAN Controller" "$USER"
        print_success "Created user: $USER"
    else
        print_warning "User $USER already exists"
    fi

    # Add user to dialout group for serial port access
    usermod -a -G dialout "$USER"
    print_success "Added $USER to dialout group"
}

# Create directories
create_directories() {
    print_status "Creating directories..."

    mkdir -p "$INSTALL_DIR/bin"
    mkdir -p "$CONFIG_DIR"
    mkdir -p "$DATA_DIR"
    mkdir -p "$LOG_DIR"

    # Set permissions
    chown -R "$USER:$GROUP" "$INSTALL_DIR"
    chown -R "$USER:$GROUP" "$CONFIG_DIR"
    chown -R "$USER:$GROUP" "$DATA_DIR"
    chown -R "$USER:$GROUP" "$LOG_DIR"

    chmod 755 "$INSTALL_DIR"
    chmod 755 "$CONFIG_DIR"
    chmod 750 "$DATA_DIR"
    chmod 750 "$LOG_DIR"

    print_success "Created and configured directories"
}

# Install binaries
install_binaries() {
    print_status "Installing binaries..."

    # Install server binary
    cp "./target/release/openfan-server" "$INSTALL_DIR/bin/"
    chmod 755 "$INSTALL_DIR/bin/openfan-server"
    chown "$USER:$GROUP" "$INSTALL_DIR/bin/openfan-server"

    # Install CLI binary to system PATH
    cp "./target/release/openfan" "/usr/local/bin/"
    chmod 755 "/usr/local/bin/openfan"

    print_success "Installed binaries"
}

# Install configuration
install_config() {
    print_status "Installing configuration..."

    if [[ ! -f "$CONFIG_DIR/config.yaml" ]]; then
        if [[ -f "./config.yaml" ]]; then
            cp "./config.yaml" "$CONFIG_DIR/"
        else
            # Create default config
            cat > "$CONFIG_DIR/config.yaml" << 'EOF'
server:
  port: 8080
  bind: "127.0.0.1"

hardware:
  device_path: "/dev/ttyUSB0"
  baud_rate: 115200
  timeout_ms: 2000

fans:
  count: 10

fan_profiles:
  "50% PWM":
    control_mode: "Pwm"
    values: [50, 50, 50, 50, 50, 50, 50, 50, 50, 50]
  "100% PWM":
    control_mode: "Pwm"
    values: [100, 100, 100, 100, 100, 100, 100, 100, 100, 100]
  "1000 RPM":
    control_mode: "Rpm"
    values: [1000, 1000, 1000, 1000, 1000, 1000, 1000, 1000, 1000, 1000]

fan_aliases:
  0: "Fan #1"
  1: "Fan #2"
  2: "Fan #3"
  3: "Fan #4"
  4: "Fan #5"
  5: "Fan #6"
  6: "Fan #7"
  7: "Fan #8"
  8: "Fan #9"
  9: "Fan #10"
EOF
        fi

        chown "$USER:$GROUP" "$CONFIG_DIR/config.yaml"
        chmod 640 "$CONFIG_DIR/config.yaml"
        print_success "Installed default configuration"
    else
        print_warning "Configuration file already exists, skipping"
    fi
}

# Install systemd service
install_service() {
    print_status "Installing systemd service..."

    if [[ -f "./deploy/openfan-server.service" ]]; then
        cp "./deploy/openfan-server.service" "/etc/systemd/system/"
        systemctl daemon-reload
        print_success "Installed systemd service"
    else
        print_error "Service file not found at ./deploy/openfan-server.service"
        exit 1
    fi
}

# Setup logrotate
setup_logrotate() {
    print_status "Setting up log rotation..."

    cat > "/etc/logrotate.d/openfan" << EOF
$LOG_DIR/*.log {
    daily
    rotate 30
    compress
    delaycompress
    missingok
    notifempty
    create 640 $USER $GROUP
    postrotate
        systemctl reload-or-restart $SERVICE_NAME
    endscript
}
EOF

    print_success "Configured log rotation"
}

# Install shell completion
install_completion() {
    print_status "Installing shell completion..."

    # Generate completion scripts
    if command -v bash &> /dev/null; then
        "$INSTALL_DIR/bin/openfan" completion bash > "/etc/bash_completion.d/openfan"
        print_success "Installed bash completion"
    fi

    if command -v zsh &> /dev/null; then
        mkdir -p "/usr/local/share/zsh/site-functions"
        "$INSTALL_DIR/bin/openfan" completion zsh > "/usr/local/share/zsh/site-functions/_openfan"
        print_success "Installed zsh completion"
    fi
}

# Test installation
test_installation() {
    print_status "Testing installation..."

    # Test CLI
    if /usr/local/bin/openfan --help &>/dev/null; then
        print_success "CLI tool working"
    else
        print_error "CLI tool test failed"
        return 1
    fi

    # Test server binary
    if "$INSTALL_DIR/bin/openfan-server" --help &>/dev/null; then
        print_success "Server binary working"
    else
        print_error "Server binary test failed"
        return 1
    fi

    # Test service file
    if systemctl is-enabled "$SERVICE_NAME" &>/dev/null || systemctl status "$SERVICE_NAME" &>/dev/null; then
        print_success "Service configuration valid"
    else
        print_success "Service ready for enabling"
    fi
}

# Print post-installation instructions
print_instructions() {
    echo
    echo "============================================"
    print_success "OpenFAN Controller installed successfully!"
    echo "============================================"
    echo
    echo "Configuration file: $CONFIG_DIR/config.yaml"
    echo "Data directory:     $DATA_DIR"
    echo "Log directory:      $LOG_DIR"
    echo
    echo "To start the service:"
    echo "  sudo systemctl enable $SERVICE_NAME"
    echo "  sudo systemctl start $SERVICE_NAME"
    echo
    echo "To check service status:"
    echo "  sudo systemctl status $SERVICE_NAME"
    echo
    echo "To view logs:"
    echo "  sudo journalctl -u $SERVICE_NAME -f"
    echo
    echo "CLI usage:"
    echo "  openfan info                    # System information"
    echo "  openfan status                  # Fan status"
    echo "  openfan fan set 0 --pwm 50      # Set fan PWM"
    echo "  openfan profile list            # List profiles"
    echo "  openfan --help                  # Show all commands"
    echo
    echo "Configuration:"
    echo "  Edit $CONFIG_DIR/config.yaml to customize settings"
    echo "  Restart service after configuration changes:"
    echo "  sudo systemctl restart $SERVICE_NAME"
    echo
    print_warning "Make sure your user is in the 'dialout' group to access hardware:"
    echo "  sudo usermod -a -G dialout \$USER"
    echo "  (then log out and back in)"
    echo
}

# Uninstall function
uninstall() {
    print_status "Uninstalling OpenFAN Controller..."

    # Stop and disable service
    systemctl stop "$SERVICE_NAME" 2>/dev/null || true
    systemctl disable "$SERVICE_NAME" 2>/dev/null || true

    # Remove files
    rm -f "/etc/systemd/system/openfan-server.service"
    rm -f "/usr/local/bin/openfan"
    rm -f "/etc/bash_completion.d/openfan"
    rm -f "/usr/local/share/zsh/site-functions/_openfan"
    rm -f "/etc/logrotate.d/openfan"
    rm -rf "$INSTALL_DIR"

    # Remove user and group
    userdel "$USER" 2>/dev/null || true
    groupdel "$GROUP" 2>/dev/null || true

    # Keep config and data directories for safety
    print_warning "Configuration and data directories preserved:"
    print_warning "  $CONFIG_DIR"
    print_warning "  $DATA_DIR"
    print_warning "  $LOG_DIR"
    print_warning "Remove manually if desired"

    systemctl daemon-reload
    print_success "OpenFAN Controller uninstalled"
}

# Main installation function
main() {
    echo "OpenFAN Controller Installation Script"
    echo "======================================"

    case "${1:-install}" in
        install)
            check_root
            check_requirements
            create_user
            create_directories
            install_binaries
            install_config
            install_service
            setup_logrotate
            install_completion
            test_installation
            print_instructions
            ;;
        uninstall)
            check_root
            uninstall
            ;;
        *)
            echo "Usage: $0 [install|uninstall]"
            echo "  install    - Install OpenFAN Controller (default)"
            echo "  uninstall  - Remove OpenFAN Controller"
            exit 1
            ;;
    esac
}

# Run main function with all arguments
main "$@"
