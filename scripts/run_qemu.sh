#!/bin/bash
# RustOS QEMU Test Script
# This script runs your RustOS kernel directly with QEMU

KERNEL_IMAGE="target/x86_64-rustos/debug/bootimage-rustos.bin"

if [ ! -f "$KERNEL_IMAGE" ]; then
    echo "Error: Kernel image not found at $KERNEL_IMAGE"
    echo "Run 'cargo bootimage --bin rustos' first to build the kernel"
    exit 1
fi

echo "Starting RustOS with QEMU..."
echo "Kernel image: $KERNEL_IMAGE"
echo ""
echo "Press Ctrl+A then X to exit QEMU"
echo "Press Ctrl+A then C to access QEMU monitor"
echo ""

# Run QEMU with your RustOS kernel
DISPLAY_MODE="${RUSTOS_QEMU_DISPLAY:-}"
if [ -z "$DISPLAY_MODE" ]; then
    if [ "$(uname -s)" = "Darwin" ]; then
        DISPLAY_MODE="cocoa"
    else
        DISPLAY_MODE="gtk"
    fi
fi

qemu-system-x86_64 \
    -drive format=raw,file="$KERNEL_IMAGE" \
    -m 256M \
    -smp 2 \
    -serial stdio \
    -display "$DISPLAY_MODE" \
    -device isa-debug-exit,iobase=0xf4,iosize=0x04 \
    -machine pc \
    -cpu qemu64,+lm,+nx \
    -enable-kvm
