#!/usr/bin/env bash
# Verify QEMU is installed before kernel test/boot-smoke targets.
set -euo pipefail

QEMU="${QEMU_SYSTEM_X86_64:-qemu-system-x86_64}"

if ! command -v "$QEMU" >/dev/null 2>&1; then
    echo "Error: $QEMU not found. Install QEMU (e.g. brew install qemu)." >&2
    exit 1
fi

if ! "$QEMU" -version >/dev/null 2>&1; then
    echo "Error: $QEMU is installed but failed to run (-version)." >&2
    exit 1
fi

exit 0
