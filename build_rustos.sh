#!/bin/bash

# RustOS Build Script
# Comprehensive build system for the RustOS kernel
# Supports x86_64 and AArch64 architectures

set -e  # Exit on any error

# Color codes for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
PURPLE='\033[0;35m'
CYAN='\033[0;36m'
WHITE='\033[1;37m'
NC='\033[0m' # No Color

# Build configuration
KERNEL_NAME="rustos"
DEFAULT_TARGET="x86_64-rustos.json"
BUILD_DIR="target"
BOOTIMAGE_DIR="bootimage"

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

print_header() {
    echo -e "${PURPLE}================================${NC}"
    echo -e "${PURPLE}$1${NC}"
    echo -e "${PURPLE}================================${NC}"
}

# Show help information
show_help() {
    cat << EOF
RustOS Build Script

Usage: $0 [OPTIONS]

OPTIONS:
    -h, --help          Show this help message
    -t, --target TARGET Set target architecture (x86_64-rustos.json, aarch64-apple-rustos.json)
    -r, --release       Build in release mode (default: debug)
    -c, --clean         Clean build artifacts before building
    -b, --bootimage     Create bootable disk image
    -q, --qemu          Run in QEMU after building
    -v, --verbose       Enable verbose output
    --install-deps      Install build dependencies
    --check-only        Only check compilation, don't build
    --test              Run kernel tests

EXAMPLES:
    $0                              # Build debug kernel for x86_64
    $0 -r -b                       # Build release kernel with bootimage
    $0 -t aarch64-apple-rustos.json # Build for AArch64
    $0 --clean -r -b -q            # Full clean release build and run in QEMU

EOF
}

# Check if command exists
command_exists() {
    command -v "$1" >/dev/null 2>&1
}

# Install Rust if not present
install_rust() {
    if ! command_exists rustc; then
        print_status "Rust not found. Installing Rust..."

        # Download and install rustup
        curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --default-toolchain nightly

        # Source the environment
        source ~/.cargo/env

        print_success "Rust installed successfully"
    else
        print_status "Rust already installed: $(rustc --version)"
    fi

    # Ensure we're using nightly
    rustup toolchain install nightly
    rustup default nightly

    # Install required components
    rustup component add rust-src
    rustup component add llvm-tools-preview
}

# Install build dependencies
install_dependencies() {
    print_header "Installing Build Dependencies"

    # Install Rust
    install_rust

    # Install bootimage cargo plugin
    if ! command_exists cargo-bootimage; then
        print_status "Installing bootimage..."
        cargo install bootimage
    fi

    # Install other useful tools
    if ! command_exists cargo-binutils; then
        print_status "Installing cargo-binutils..."
        cargo install cargo-binutils
    fi

    # Install QEMU if available (for testing)
    if command_exists apt-get; then
        print_status "Installing QEMU (Ubuntu/Debian)..."
        sudo apt-get update
        sudo apt-get install -y qemu-system-x86 qemu-system-aarch64
    elif command_exists yum; then
        print_status "Installing QEMU (RedHat/CentOS)..."
        sudo yum install -y qemu-system-x86 qemu-system-aarch64
    elif command_exists brew; then
        print_status "Installing QEMU (macOS)..."
        brew install qemu
    elif command_exists pacman; then
        print_status "Installing QEMU (Arch Linux)..."
        sudo pacman -S qemu-arch-extra
    else
        print_warning "Could not detect package manager. Please install QEMU manually for testing."
    fi

    print_success "Dependencies installed"
}

# Clean build artifacts
clean_build() {
    print_status "Cleaning build artifacts..."

    if [ -d "$BUILD_DIR" ]; then
        rm -rf "$BUILD_DIR"
        print_success "Removed build directory"
    fi

    if [ -d "$BOOTIMAGE_DIR" ]; then
        rm -rf "$BOOTIMAGE_DIR"
        print_success "Removed bootimage directory"
    fi

    cargo clean 2>/dev/null || true
}

# Validate build environment
validate_environment() {
    print_status "Validating build environment..."

    # Check Rust installation
    if ! command_exists rustc; then
        print_error "Rust compiler not found. Run with --install-deps"
        exit 1
    fi

    # Check target file exists
    if [ ! -f "$TARGET" ]; then
        print_error "Target specification file not found: $TARGET"
        exit 1
    fi

    # Check Cargo.toml
    if [ ! -f "Cargo.toml" ]; then
        print_error "Cargo.toml not found. Run this script from the project root."
        exit 1
    fi

    print_success "Environment validation passed"
}

# Build kernel
build_kernel() {
    print_header "Building RustOS Kernel"

    local build_args=""
    local cargo_unstable="-Zjson-target-spec"
    local target_flag="--target $TARGET"

    if [ "$RELEASE" = true ]; then
        build_args="--release"
        print_status "Building in RELEASE mode"
    else
        print_status "Building in DEBUG mode"
    fi

    if [ "$VERBOSE" = true ]; then
        build_args="$build_args --verbose"
    fi

    print_status "Target: $TARGET"
    print_status "Build arguments: $build_args $target_flag"

    # Set required environment variables
    export RUST_TARGET_PATH="$(pwd)"

    # Build the kernel
    if [ "$CHECK_ONLY" = true ]; then
        print_status "Checking compilation only..."
        cargo check -p rustos --bin rustos $cargo_unstable -Zbuild-std=core,compiler_builtins,alloc $target_flag $build_args
        print_success "Compilation check passed"
        return 0
    fi

    print_status "Compiling kernel..."
    cargo build -p rustos --bin rustos $cargo_unstable -Zbuild-std=core,compiler_builtins,alloc $target_flag $build_args

    local binary_name="rustos"
    # Extract target name without .json extension for path
    local target_path="${TARGET%.json}"
    if [ "$RELEASE" = true ]; then
        KERNEL_BINARY="$BUILD_DIR/$target_path/release/$binary_name"
    else
        KERNEL_BINARY="$BUILD_DIR/$target_path/debug/$binary_name"
    fi

    if [ -f "$KERNEL_BINARY" ]; then
        print_success "Kernel built successfully: $KERNEL_BINARY"

        # Show binary information
        local size=$(ls -lh "$KERNEL_BINARY" | awk '{print $5}')
        print_status "Kernel size: $size"
    else
        print_error "Kernel binary not found at: $KERNEL_BINARY"
        exit 1
    fi
}

# Create bootable image
create_bootimage() {
    print_header "Creating Bootable Image"

    print_status "Creating bootable disk image with cargo-bootimage..."

    if ! command_exists cargo-bootimage; then
        print_error "cargo-bootimage not found. Install with: cargo install bootimage"
        exit 1
    fi

    local build_args=""
    if [ "$RELEASE" = true ]; then
        build_args="--release"
    fi

    print_status "Building bootimage for target: $TARGET"
    cargo bootimage -Zjson-target-spec -Zbuild-std=core,compiler_builtins,alloc --target "$TARGET" $build_args

    local target_path="${TARGET%.json}"
    local profile_dir="debug"
    if [ "$RELEASE" = true ]; then
        profile_dir="release"
    fi
    BOOTIMAGE_PATH="$BUILD_DIR/$target_path/$profile_dir/bootimage-${KERNEL_NAME}.bin"

    if [ ! -f "$BOOTIMAGE_PATH" ]; then
        print_error "Bootimage not found at: $BOOTIMAGE_PATH"
        exit 1
    fi

    print_success "Bootimage created: $BOOTIMAGE_PATH"
    local size=$(ls -lh "$BOOTIMAGE_PATH" | awk '{print $5}')
    print_status "Bootimage size: $size"

    # Calculate checksums if available
    if command_exists sha256sum; then
        local checksum=$(sha256sum "$BOOTIMAGE_PATH" | cut -d' ' -f1)
        print_status "Bootimage SHA256: $checksum"
    fi
}

# Run tests
run_tests() {
    print_header "Running Kernel Tests"

    print_status "Running unit tests..."
    cargo test -Zjson-target-spec -Zbuild-std=core,compiler_builtins,alloc --target "$TARGET" --bin "$KERNEL_NAME"

    print_success "All tests passed"
}

# Run in QEMU
run_qemu() {
    print_header "Running RustOS in QEMU"

    if [ ! -f "$BOOTIMAGE_PATH" ]; then
        print_error "Bootimage not found. Create bootimage first with -b option."
        exit 1
    fi

    if ! command_exists qemu-system-x86_64 && [[ "$TARGET" == *"x86_64"* ]]; then
        print_error "QEMU not found. Install QEMU to test the kernel."
        exit 1
    fi

    print_status "Starting QEMU with bootloader_api support..."
    print_status "Press Ctrl+A, then X to exit QEMU"
    print_status "Press Ctrl+A, then C for QEMU monitor"

    # Enhanced QEMU args for bootloader-based testing
    local qemu_args="-drive format=raw,file=$BOOTIMAGE_PATH"
    qemu_args="$qemu_args -serial stdio"
    qemu_args="$qemu_args -device isa-debug-exit,iobase=0xf4,iosize=0x04"
    local display_mode="${RUSTOS_QEMU_DISPLAY:-}"
    if [ -z "$display_mode" ]; then
        if [ "$(uname -s)" = "Darwin" ]; then
            display_mode="cocoa"
        else
            display_mode="gtk"
        fi
    fi
    qemu_args="$qemu_args -display $display_mode"
    qemu_args="$qemu_args -m 512M"  # Increased memory for ACPI/PCI testing
    qemu_args="$qemu_args -cpu qemu64,+apic"  # Enable APIC for ACPI testing
    qemu_args="$qemu_args -machine pc,accel=tcg"  # PC chipset (q35 causes SMI triple fault with bootloader)

    if [[ "$TARGET" == *"x86_64"* ]]; then
        qemu-system-x86_64 $qemu_args
    elif [[ "$TARGET" == *"aarch64"* ]]; then
        qemu-system-aarch64 -machine virt $qemu_args
    else
        print_error "Unsupported target for QEMU: $TARGET"
        exit 1
    fi
}

# Show build summary
show_summary() {
    print_header "Build Summary"

    echo -e "${CYAN}Project:${NC} $KERNEL_NAME"
    echo -e "${CYAN}Target:${NC} $TARGET"
    echo -e "${CYAN}Build Mode:${NC} $([ "$RELEASE" = true ] && echo "Release" || echo "Debug")"

    if [ -f "$KERNEL_BINARY" ]; then
        local size=$(ls -lh "$KERNEL_BINARY" | awk '{print $5}')
        echo -e "${CYAN}Kernel Binary:${NC} $KERNEL_BINARY ($size)"
    fi

    if [ -f "$BOOTIMAGE_PATH" ]; then
        local size=$(ls -lh "$BOOTIMAGE_PATH" | awk '{print $5}')
        echo -e "${CYAN}Boot Image:${NC} $BOOTIMAGE_PATH ($size)"
    fi

    echo -e "${CYAN}Rust Version:${NC} $(rustc --version)"
    echo -e "${CYAN}Build Time:${NC} $(date)"
}

# Parse command line arguments
TARGET="$DEFAULT_TARGET"
RELEASE=false
CLEAN=false
CREATE_BOOTIMAGE=false
RUN_QEMU=false
VERBOSE=false
INSTALL_DEPS=false
CHECK_ONLY=false
RUN_TESTS=false

while [[ $# -gt 0 ]]; do
    case $1 in
        -h|--help)
            show_help
            exit 0
            ;;
        -t|--target)
            TARGET="$2"
            shift 2
            ;;
        -r|--release)
            RELEASE=true
            shift
            ;;
        -c|--clean)
            CLEAN=true
            shift
            ;;
        -b|--bootimage)
            CREATE_BOOTIMAGE=true
            shift
            ;;
        -q|--qemu)
            RUN_QEMU=true
            CREATE_BOOTIMAGE=true  # Bootimage required for QEMU
            shift
            ;;
        -v|--verbose)
            VERBOSE=true
            shift
            ;;
        --install-deps)
            INSTALL_DEPS=true
            shift
            ;;
        --check-only)
            CHECK_ONLY=true
            shift
            ;;
        --test)
            RUN_TESTS=true
            shift
            ;;
        *)
            print_error "Unknown option: $1"
            echo "Use --help for usage information"
            exit 1
            ;;
    esac
done

# Main build process
main() {
    print_header "RustOS Kernel Build System"

    # Install dependencies if requested
    if [ "$INSTALL_DEPS" = true ]; then
        install_dependencies
    fi

    # Validate environment
    validate_environment

    # Clean if requested
    if [ "$CLEAN" = true ]; then
        clean_build
    fi

    # Run tests if requested
    if [ "$RUN_TESTS" = true ]; then
        run_tests
    fi

    # Build kernel
    build_kernel

    # Create bootimage if requested
    if [ "$CREATE_BOOTIMAGE" = true ] && [ "$CHECK_ONLY" = false ]; then
        create_bootimage
    fi

    # Run in QEMU if requested
    if [ "$RUN_QEMU" = true ]; then
        run_qemu
    fi

    # Show summary
    show_summary

    print_success "Build process completed successfully!"
}

# Run main function
main "$@"
