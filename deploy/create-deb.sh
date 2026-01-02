#!/bin/bash
# OpenFAN Controller Debian Package Builder
# This script creates a .deb package for easy installation on Debian/Ubuntu systems

set -e  # Exit on any error

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Configuration
PACKAGE_NAME="openfan-controller"
VERSION=$(grep '^version' Cargo.toml | head -1 | sed 's/.*"\(.*\)".*/\1/')
ARCHITECTURE="amd64"
PACKAGE_DIR="debian-package"
DEB_DIR="$PACKAGE_DIR/DEBIAN"

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

# Check if we're in the right directory
check_environment() {
  if [[ ! -f "Cargo.toml" ]] || [[ ! -d "openfand" ]] || [[ ! -d "openfanctl" ]]; then
    print_error "This script must be run from the openfan workspace root directory"
    exit 1
  fi
}

# Check build requirements
check_requirements() {
  print_status "Checking requirements..."

  if ! command -v dpkg-deb &> /dev/null; then
    print_error "dpkg-deb not found. Please install dpkg-dev package"
    exit 1
  fi

  if [[ ! -f "target/release/openfand" ]] || [[ ! -f "target/release/openfanctl" ]]; then
    print_error "Release binaries not found. Please run 'cargo build --release' first"
    exit 1
  fi

  print_success "Requirements satisfied"
}

# Clean previous package builds
clean_package() {
  print_status "Cleaning previous package builds..."
  rm -rf "$PACKAGE_DIR"
  rm -f "${PACKAGE_NAME}_${VERSION}_${ARCHITECTURE}.deb"
  print_success "Cleaned package environment"
}

# Create package directory structure
create_package_structure() {
  print_status "Creating package structure..."

  # Create base directories
  mkdir -p "$DEB_DIR"
  mkdir -p "$PACKAGE_DIR/opt/openfan/bin"
  mkdir -p "$PACKAGE_DIR/etc/openfan"
  mkdir -p "$PACKAGE_DIR/etc/systemd/system"
  mkdir -p "$PACKAGE_DIR/usr/local/bin"
  mkdir -p "$PACKAGE_DIR/etc/bash_completion.d"
  mkdir -p "$PACKAGE_DIR/usr/share/doc/$PACKAGE_NAME"
  mkdir -p "$PACKAGE_DIR/var/lib/openfan"
  mkdir -p "$PACKAGE_DIR/var/log/openfan"

  print_success "Package structure created"
}

# Copy binaries and files
copy_files() {
  print_status "Copying files..."

  # Copy binaries
  cp target/release/openfand "$PACKAGE_DIR/opt/openfan/bin/"
  cp target/release/openfanctl "$PACKAGE_DIR/usr/local/bin/"

  # Copy configuration
  cp config.toml "$PACKAGE_DIR/etc/openfan/"

  # Copy systemd service
  cp deploy/openfand.service "$PACKAGE_DIR/etc/systemd/system/"

  # Copy documentation
  cp README.md "$PACKAGE_DIR/usr/share/doc/$PACKAGE_NAME/"

  # Generate bash completion
  target/release/openfanctl completion bash > "$PACKAGE_DIR/etc/bash_completion.d/openfanctl" || true

  print_success "Files copied"
}

# Create control file
create_control_file() {
  print_status "Creating control file..."

  # Get installed size (in KB)
  local installed_size
  installed_size=$(du -sk "$PACKAGE_DIR" | cut -f1)

    cat > "$DEB_DIR/control" << EOF
Package: $PACKAGE_NAME
Version: $VERSION
Section: utils
Priority: optional
Architecture: $ARCHITECTURE
Installed-Size: $installed_size
Depends: libc6 (>= 2.17), libgcc-s1 (>= 3.0), systemd
Maintainer: OpenFAN Contributors <maintainer@example.com>
Homepage: https://github.com/graelo/openfan-rs
Description: OpenFAN Controller - Fan Management System
 OpenFAN Controller is a high-performance fan management system written in Rust.
 It provides both a REST API server and command-line interface for controlling
 computer fans via serial communication.
 .
 Features:
  - REST API server for remote fan control
  - Command-line interface with intuitive commands
  - Support for PWM and RPM control modes
  - Fan profiles for automated control
  - Real-time monitoring and status reporting
  - Hardware auto-detection
  - Mock mode for testing without hardware
 .
 This package includes:
  - openfand: REST API server daemon
  - openfanctl: Command-line interface tool
  - systemd service configuration
  - Default configuration files
EOF

  print_success "Control file created"
}

# Create post-installation script
create_postinst() {
  print_status "Creating post-installation script..."

    cat > "$DEB_DIR/postinst" << 'EOF'
#!/bin/bash
set -e

# Create user and group
if ! getent group openfan >/dev/null 2>&1; then
  groupadd --system openfan
fi

if ! getent passwd openfan >/dev/null 2>&1; then
  useradd --system --gid openfan --shell /bin/false \
    --home-dir /var/lib/openfan --create-home \
    --comment "OpenFAN Controller" openfan
fi

# Add user to dialout group for serial port access
usermod -a -G dialout openfan

# Set permissions
chown -R openfan:openfan /opt/openfan
chown -R openfan:openfan /etc/openfan
chown -R openfan:openfan /var/lib/openfan
chown -R openfan:openfan /var/log/openfan

chmod 755 /opt/openfan
chmod 755 /etc/openfan
chmod 750 /var/lib/openfan
chmod 750 /var/log/openfan
chmod 640 /etc/openfan/config.toml

# Set binary permissions
chmod 755 /opt/openfan/bin/openfand
chmod 755 /usr/local/bin/openfanctl

# Reload systemd
systemctl daemon-reload

# Enable service (but don't start it automatically)
if systemctl is-enabled openfand >/dev/null 2>&1; then
  echo "Service already enabled"
else
  echo "To enable the service, run: sudo systemctl enable openfand"
fi

echo "OpenFAN Controller installed successfully!"
echo "Configuration: /etc/openfan/config.toml"
echo "To start: sudo systemctl start openfand"
echo "CLI usage: openfanctl --help"
EOF

  chmod 755 "$DEB_DIR/postinst"
  print_success "Post-installation script created"
}

# Create pre-removal script
create_prerm() {
  print_status "Creating pre-removal script..."

    cat > "$DEB_DIR/prerm" << 'EOF'
#!/bin/bash
set -e

# Stop service if running
if systemctl is-active openfand >/dev/null 2>&1; then
  systemctl stop openfand
fi

# Disable service if enabled
if systemctl is-enabled openfand >/dev/null 2>&1; then
  systemctl disable openfand
fi
EOF

  chmod 755 "$DEB_DIR/prerm"
  print_success "Pre-removal script created"
}

# Create post-removal script
create_postrm() {
  print_status "Creating post-removal script..."

    cat > "$DEB_DIR/postrm" << 'EOF'
#!/bin/bash
set -e

case "$1" in
  purge)
    # Remove user and group
    if getent passwd openfan >/dev/null 2>&1; then
      userdel openfan
    fi
    if getent group openfan >/dev/null 2>&1; then
      groupdel openfan
    fi

    # Remove data directories
    rm -rf /var/lib/openfan
    rm -rf /var/log/openfan
    rm -rf /etc/openfan

    echo "OpenFAN Controller completely removed"
    ;;
  remove)
    # Reload systemd
    systemctl daemon-reload
    echo "OpenFAN Controller removed (config preserved)"
    ;;
esac
EOF

  chmod 755 "$DEB_DIR/postrm"
  print_success "Post-removal script created"
}

# Set file permissions
set_permissions() {
  print_status "Setting file permissions..."

  # Set binary permissions
  chmod 755 "$PACKAGE_DIR/opt/openfan/bin/openfand"
  chmod 755 "$PACKAGE_DIR/usr/local/bin/openfanctl"

  # Set config permissions
  chmod 644 "$PACKAGE_DIR/etc/openfan/config.toml"
  chmod 644 "$PACKAGE_DIR/etc/systemd/system/openfand.service"

  # Set documentation permissions
  chmod 644 "$PACKAGE_DIR/usr/share/doc/$PACKAGE_NAME/README.md"

  print_success "Permissions set"
}

# Build the package
build_package() {
  print_status "Building Debian package..."

  dpkg-deb --build "$PACKAGE_DIR" "${PACKAGE_NAME}_${VERSION}_${ARCHITECTURE}.deb"

  print_success "Package built: ${PACKAGE_NAME}_${VERSION}_${ARCHITECTURE}.deb"
}

# Verify the package
verify_package() {
  print_status "Verifying package..."

  # Check package info
  dpkg-deb --info "${PACKAGE_NAME}_${VERSION}_${ARCHITECTURE}.deb"

  # List package contents
  echo
  print_status "Package contents:"
  dpkg-deb --contents "${PACKAGE_NAME}_${VERSION}_${ARCHITECTURE}.deb"

  print_success "Package verification completed"
}

# Print installation instructions
print_instructions() {
  echo
  echo "============================================"
  print_success "Debian package created successfully!"
  echo "============================================"
  echo
  echo "Package: ${PACKAGE_NAME}_${VERSION}_${ARCHITECTURE}.deb"
  echo "Size:    $(du -h "${PACKAGE_NAME}_${VERSION}_${ARCHITECTURE}.deb" | cut -f1)"
  echo
  echo "Installation:"
  echo "  sudo dpkg -i ${PACKAGE_NAME}_${VERSION}_${ARCHITECTURE}.deb"
  echo "  sudo apt-get install -f  # Fix dependencies if needed"
  echo
  echo "Start service:"
  echo "  sudo systemctl enable openfand"
  echo "  sudo systemctl start openfand"
  echo
  echo "Usage:"
  echo "  openfan --help"
  echo "  openfan info"
  echo "  openfan status"
  echo
  echo "Configuration:"
  echo "  /etc/openfan/config.toml"
  echo
  echo "Logs:"
  echo "  sudo journalctl -u openfand"
  echo
  echo "Uninstall:"
  echo "  sudo apt remove $PACKAGE_NAME"
  echo "  sudo apt purge $PACKAGE_NAME  # Remove all files"
  echo
}

# Main function
main() {
  echo "OpenFAN Controller Debian Package Builder"
  echo "========================================="
  echo "Version: $VERSION"
  echo

  check_environment
  check_requirements
  clean_package
  create_package_structure
  copy_files
  create_control_file
  create_postinst
  create_prerm
  create_postrm
  set_permissions
  build_package
  verify_package
  print_instructions
}

# Run main function
main "$@"
