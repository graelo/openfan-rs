#!/bin/bash
# OpenFAN Controller Release Build Script
# This script builds optimized release binaries for distribution

set -e  # Exit on any error

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Configuration
PROJECT_NAME="openfan"
VERSION=$(grep '^version' Cargo.toml | head -1 | sed 's/.*"\(.*\)".*/\1/')
BUILD_DIR="dist"
TARGET_DIR="target/release"

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

# Clean previous builds
clean_build() {
    print_status "Cleaning previous builds..."
    cargo clean
    rm -rf "$BUILD_DIR"
    mkdir -p "$BUILD_DIR"
    print_success "Cleaned build environment"
}

# Check system requirements
check_requirements() {
    print_status "Checking build requirements..."

    # Check Rust
    if ! command -v cargo &> /dev/null; then
        print_error "Rust/Cargo not found. Please install Rust first."
        exit 1
    fi

    # Check version
    local rust_version
    rust_version=$(rustc --version)
    print_status "Using Rust: $rust_version"

    # Check for release profile optimization
    if ! grep -q '\[profile.release\]' Cargo.toml; then
        print_warning "No release profile optimization found in Cargo.toml"
    fi

    print_success "Build requirements satisfied"
}

# Build optimized release binaries
build_release() {
    print_status "Building optimized release binaries..."
    print_status "This may take several minutes..."

    # Build with optimizations
    RUSTFLAGS="-C target-cpu=native" cargo build --release

    # Verify binaries were created
if [[ ! -f "$TARGET_DIR/openfand" ]]; then
        print_error "Server binary not found after build"
        exit 1
    fi

    if [[ ! -f "$TARGET_DIR/openfanctl" ]]; then
        print_error "CLI binary not found after build"
        exit 1
    fi

    print_success "Release binaries built successfully"
}

# Run tests
run_tests() {
    print_status "Running test suite..."

    # Run unit tests
    cargo test --lib --release

    # Run integration tests
    cargo test --test simple_integration_tests --release

    print_success "All tests passed"
}

# Strip binaries for size optimization
strip_binaries() {
    print_status "Optimizing binary sizes..."

    if command -v strip &> /dev/null; then
        strip "$TARGET_DIR/openfand"
        strip "$TARGET_DIR/openfanctl"
        print_success "Binaries stripped"
    else
        print_warning "strip command not found, skipping binary optimization"
    fi
}

# Get binary information
get_binary_info() {
    print_status "Binary information:"

    local server_size=$(du -h "$TARGET_DIR/openfand" | cut -f1)
    local cli_size=$(du -h "$TARGET_DIR/openfanctl" | cut -f1)

    echo "  openfand: $server_size"
    echo "  openfanctl:        $cli_size"

if command -v file &> /dev/null; then
        echo "  Server arch:    $(file "$TARGET_DIR/openfand" | cut -d: -f2 | xargs)"
        echo "  CLI arch:       $(file "$TARGET_DIR/openfanctl" | cut -d: -f2 | xargs)"
    fi
}

# Create distribution package
create_distribution() {
    print_status "Creating distribution package..."

    # Copy binaries
cp "$TARGET_DIR/openfand" "$BUILD_DIR/"
    cp "$TARGET_DIR/openfanctl" "$BUILD_DIR/"

    # Copy configuration files
    if [[ -f config.toml ]]; then
        cp config.toml "$BUILD_DIR/"
    else
        print_warning "config.toml not found, skipping."
    fi
    if [[ -f README.md ]]; then
        cp README.md "$BUILD_DIR/"
    else
        print_warning "README.md not found, skipping."
    fi


    # Copy deployment files (only necessary files)
    mkdir -p "$BUILD_DIR/deploy"
    if [[ -f "deploy/openfand.service" ]]; then
        cp "deploy/openfand.service" "$BUILD_DIR/deploy/"
    else
        print_warning "deploy/openfand.service not found, skipping."
    fi
    # Add more files here if needed, e.g. udev rules


    # Create version file
    cat > "$BUILD_DIR/VERSION" << EOF
OpenFAN Controller v$VERSION
Built on: $(date -u +"%Y-%m-%d %H:%M:%S UTC")
Commit: $(git rev-parse --short HEAD 2>/dev/null || echo "unknown")
Rust: $(rustc --version)
EOF

    # Create checksums
    cd "$BUILD_DIR"
    if command -v sha256sum &> /dev/null; then
sha256sum openfand openfanctl > checksums.sha256
        print_success "Created SHA256 checksums"
    fi
    cd ..

    print_success "Distribution package created in $BUILD_DIR/"
}

# Create archive
create_archive() {
    print_status "Creating release archive..."

    local archive_name="${PROJECT_NAME}-${VERSION}-$(uname -m)-$(uname -s | tr '[:upper:]' '[:lower:]')"

    if command -v tar &> /dev/null; then
        tar -czf "${archive_name}.tar.gz" -C "$BUILD_DIR" .
        print_success "Created archive: ${archive_name}.tar.gz"
    else
        print_warning "tar not found, skipping archive creation"
    fi
}

# Verify release package
verify_release() {
    print_status "Verifying release package..."

    # Test server binary
if "$BUILD_DIR/openfand" --help &>/dev/null; then
        print_success "Server binary verified"
    else
        print_error "Server binary verification failed"
        exit 1
    fi

    # Test CLI binary
    if "$BUILD_DIR/openfanctl" --help &>/dev/null; then
        print_success "CLI binary verified"
    else
        print_error "CLI binary verification failed"
        exit 1
    fi

    # Verify checksums if available
    if [[ -f "$BUILD_DIR/checksums.sha256" ]]; then
        cd "$BUILD_DIR"
        if sha256sum -c checksums.sha256 &>/dev/null; then
            print_success "Checksums verified"
        else
            print_error "Checksum verification failed"
            exit 1
        fi
        cd ..
    fi
}

# Print release information
print_release_info() {
    echo
    echo "============================================"
    print_success "Release build completed successfully!"
    echo "============================================"
    echo
    echo "Version:     $VERSION"
    echo "Build dir:   $BUILD_DIR/"
    echo "Archive:     ${PROJECT_NAME}-${VERSION}-$(uname -m)-$(uname -s | tr '[:upper:]' '[:lower:]').tar.gz"
    echo
    echo "Contents:"
    ls -la "$BUILD_DIR/"
    echo
    echo "Installation:"
    echo "  1. Extract the archive to your target system"
    echo "  2. Run: sudo ./deploy/install.sh"
echo "  3. Start service: sudo systemctl enable --now openfand"
    echo
    echo "Manual installation:"
    echo "  1. Copy binaries to desired location"
    echo "  2. Copy config.toml to /etc/openfan/"
echo "  3. Set up systemd service using deploy/openfand.service"
    echo
    print_warning "Note: This build is optimized for $(uname -m) architecture"
    echo
}

# Cross-compilation helper
cross_compile() {
    local target="$1"
    if [[ -z "$target" ]]; then
        print_error "Target architecture required for cross compilation"
        echo "Example: $0 cross x86_64-unknown-linux-gnu"
        exit 1
    fi

    print_status "Cross-compiling for target: $target"

    # Install target if needed
    rustup target add "$target"

    # Build for target
    cargo build --release --target "$target"

    # Create target-specific distribution
    local target_dir="$BUILD_DIR/$target"
    mkdir -p "$target_dir"
cp "target/$target/release/openfand" "$target_dir/"
    cp "target/$target/release/openfanctl" "$target_dir/"

    print_success "Cross-compilation completed for $target"
}

# Main function
main() {
    echo "OpenFAN Controller Release Build Script"
    echo "======================================="
    echo "Version: $VERSION"
    echo

    case "${1:-build}" in
        build)
            check_environment
            check_requirements
            clean_build
            build_release
            run_tests
            strip_binaries
            get_binary_info
            create_distribution
            create_archive
            verify_release
            print_release_info
            ;;
        cross)
            check_environment
            check_requirements
            cross_compile "$2"
            ;;
        clean)
            clean_build
            print_success "Build environment cleaned"
            ;;
        test)
            check_environment
            run_tests
            ;;
        *)
            echo "Usage: $0 [command] [options]"
            echo "Commands:"
            echo "  build           - Build optimized release (default)"
            echo "  cross <target>  - Cross-compile for target architecture"
            echo "  clean           - Clean build environment"
            echo "  test            - Run test suite only"
            echo
            echo "Examples:"
            echo "  $0                              # Build release"
            echo "  $0 cross x86_64-unknown-linux-gnu"
            echo "  $0 cross aarch64-unknown-linux-gnu"
            exit 1
            ;;
    esac
}

# Run main function with all arguments
main "$@"

# vim: set ts=4 sts=4 sw=4 expandtab:
