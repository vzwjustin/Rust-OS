#!/bin/bash
# Build userspace/initramfs.cpio.gz from userspace/rootfs.

set -euo pipefail

ROOTFS="${1:-userspace/rootfs}"
OUTPUT="${2:-userspace/initramfs.cpio.gz}"

if ! command -v cpio >/dev/null 2>&1; then
    echo "Error: cpio not found. Install cpio (e.g. apt-get install cpio)." >&2
    exit 1
fi

if [ ! -d "$ROOTFS" ]; then
    echo "Error: rootfs directory not found at $ROOTFS" >&2
    exit 1
fi

mkdir -p "$(dirname "$OUTPUT")"
(
    cd "$ROOTFS"
    find . -print0 | cpio --null --create --verbose --format=newc
) | gzip -9 > "$OUTPUT"

echo "Built initramfs: $OUTPUT ($(wc -c < "$OUTPUT") bytes)"
