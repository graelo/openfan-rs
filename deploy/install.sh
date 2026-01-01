#!/bin/bash
# OpenFAN Controller Installation Script
# Usage: Run after building the project with 'cargo build --release'.
# This script installs the OpenFAN server and CLI tools.


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
SERVICE_NAME="openfand"

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
    if [[ ! -f "./target/release/openfand" ]]; then
        print_error "openfand binary not found. Please run 'cargo build --release' first"
        exit 1
    fi

    if [[ ! -f "./target/release/openfanctl" ]]; then
        print_error "openfanctl CLI binary not found. Please run 'cargo build --release' first"
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
    cp "./target/release/openfand" "$INSTALL_DIR/bin/"
    chmod 755 "$INSTALL_DIR/bin/openfand"
    chown "$USER:$GROUP" "$INSTALL_DIR/bin/openfand"

    # Install CLI binary to OpenFAN bin directory
    cp "./target/release/openfanctl" "$INSTALL_DIR/bin/"
    chmod 755 "$INSTALL_DIR/bin/openfanctl"


    print_success "Installed binaries"
}

# Install configuration
install_config() {
    print_status "Installing configuration..."

    if [[ ! -f "$CONFIG_DIR/config.toml" ]]; then
        if [[ -f "./config.toml" ]]; then
            cp "./config.toml" "$CONFIG_DIR/"
        else
            # Create default config
            cat > "$CONFIG_DIR/config.toml" << 'EOF'
# OpenFAN Controller Configuration
#
# This file configures the OpenFAN daemon (openfand).
#
# For mutable data (profiles, aliases, zones, thermal curves, CFM mappings),
# see the data_dir location. Hardware detection is automatic via USB VID/PID,
# OPENFAN_COMPORT environment variable, or common device paths.

# Directory for mutable data files (profiles, aliases, zones, thermal curves, CFM mappings)
# Default: ~/.local/share/openfan (XDG) or /var/lib/openfan (system)
data_dir = "/var/lib/openfan"

[server]
# Address to bind to ("localhost" or "0.0.0.0" for all interfaces)
hostname = "localhost"
# Server port
port = 3000
# Communication timeout in seconds
communication_timeout = 1
EOF
        fi

        chown "$USER:$GROUP" "$CONFIG_DIR/config.toml"
        chmod 640 "$CONFIG_DIR/config.toml"
        print_success "Installed default configuration"
    else
        print_warning "Configuration file already exists, skipping"
    fi
}

# Install systemd service
install_service() {
    print_status "Installing systemd service..."

    if [[ -f "./deploy/openfand.service" ]]; then
        cp "./deploy/openfand.service" "/etc/systemd/system/"
        systemctl daemon-reload
        print_success "Installed systemd service"
    else
      print_error "Service file not found at ./deploy/openfand.service"
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

    # Generate completion scripts for both openfanctl and openfand
    if command -v bash &> /dev/null; then
        "$INSTALL_DIR/bin/openfanctl" completion bash > "/etc/bash_completion.d/openfanctl"
        chown root:root "/etc/bash_completion.d/openfanctl"
        print_success "Installed bash completion for openfanctl"
        "$INSTALL_DIR/bin/openfand" completion bash > "/etc/bash_completion.d/openfand"
        chown root:root "/etc/bash_completion.d/openfand"
        print_success "Installed bash completion for openfand"
    fi

    if command -v zsh &> /dev/null; then
        mkdir -p "/usr/local/share/zsh/site-functions"
        "$INSTALL_DIR/bin/openfanctl" completion zsh > "/usr/local/share/zsh/site-functions/_openfanctl"
        chown root:root "/usr/local/share/zsh/site-functions/_openfanctl"
        print_success "Installed zsh completion for openfanctl"
        "$INSTALL_DIR/bin/openfand" completion zsh > "/usr/local/share/zsh/site-functions/_openfand"
        chown root:root "/usr/local/share/zsh/site-functions/_openfand"
        print_success "Installed zsh completion for openfand"
    fi


}

# Test installation
test_installation() {
    print_status "Testing installation..."

    # Test CLI
    if "$INSTALL_DIR/bin/openfanctl" --help &>/dev/null; then
        print_success "CLI tool working"
    else
        print_error "CLI tool test failed"
        return 1
    fi


    # Test server binary
if "$INSTALL_DIR/bin/openfand" --help &>/dev/null; then
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
    echo "Configuration file: $CONFIG_DIR/config.toml"
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
    echo "CLI usage (openfanctl):"
    echo "  openfanctl info                    # System information"
    echo "  openfanctl status                  # Fan status"
    echo "  openfanctl fan set 0 --pwm 50      # Set fan PWM"
    echo "  openfanctl profile list            # List profiles"
    echo "  openfanctl --help                  # Show all commands"
    echo
    echo "Configuration:"
    echo "  Edit $CONFIG_DIR/config.toml to customize settings"
    echo "  Restart service after configuration changes:"
    echo "  sudo systemctl restart $SERVICE_NAME"
    echo
    echo "Shell completion:"
    echo "  Bash: source /etc/bash_completion.d/openfanctl or openfand"
    echo "  Zsh:  source /usr/local/share/zsh/site-functions/_openfanctl or _openfand"
    echo
    echo "Note: To run openfanctl or openfand from anywhere, add $INSTALL_DIR/bin to your PATH:"
    echo "  export PATH=\"$INSTALL_DIR/bin:\$PATH\""
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
rm -f "/etc/systemd/system/openfand.service"
    rm -f "$INSTALL_DIR/bin/openfanctl"
    rm -f "$INSTALL_DIR/bin/openfand"


    rm -f "/etc/bash_completion.d/openfanctl"
    rm -f "/etc/bash_completion.d/openfand"
    rm -f "/usr/local/share/zsh/site-functions/_openfanctl"
    rm -f "/usr/local/share/zsh/site-functions/_openfand"

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
