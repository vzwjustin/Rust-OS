#!/bin/bash
# RustOS Debug Script
# This will help diagnose boot issues

KERNEL_IMAGE="target/x86_64-rustos/debug/bootimage-rustos.bin"

if [ ! -f "$KERNEL_IMAGE" ]; then
    echo "Error: Kernel image not found at $KERNEL_IMAGE"
    echo "Run 'cargo bootimage --bin rustos' first to build the kernel"
    exit 1
fi

echo "=== RustOS Boot Debug Information ==="
echo "Kernel image: $KERNEL_IMAGE"
echo "File size: $(stat -f%z "$KERNEL_IMAGE") bytes"
echo ""

echo "=== Running QEMU with verbose output ==="
echo "Look for any error messages or boot information..."
echo ""

# Run QEMU with serial output to see what's happening
qemu-system-x86_64 \
    -drive format=raw,file="$KERNEL_IMAGE" \
    -m 256M \
    -smp 1 \
    -serial stdio \
    -display cocoa \
    -device isa-debug-exit,iobase=0xf4,iosize=0x04 \
    -machine pc \
    -cpu Nehalem \
    -d cpu_reset -D qemu.log \
    2>&1 | tee boot_output.log
