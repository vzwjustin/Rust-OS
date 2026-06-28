#!/bin/bash
# Refresh the embedded rootfs initramfs after GNOME packages are installed on RustOS.
#
# Primary workflow (no separate Linux host required):
#   1. make run
#   2. Inside RustOS: rustos-install-gnome   (uses /sbin/apk via linux compat)
#   3. On the host: make initramfs && make bootimage
#
# Optional: pre-seed packages on the host with apk --root userspace/rootfs (Linux only).

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
ROOTFS="$PROJECT_DIR/userspace/rootfs"

echo "=== RustOS GNOME rootfs ==="
echo "RustOS runs Alpine userspace through linux_compat (fork/exec/connect/mmap)."
echo ""
echo "In-system install (recommended):"
echo "  make run"
echo "  # at RustOS prompt: rustos-install-gnome"
echo "  # then on host:     make initramfs && make bootimage"
echo ""

if [ "$(uname -s)" = "Linux" ] && command -v apk >/dev/null 2>&1 && [ "${RUSTOS_APK_ROOTFS:-0}" = "1" ]; then
    echo "=== Host-side apk --root install (RUSTOS_APK_ROOTFS=1) ==="
    sudo apk \
        --arch x86_64 \
        --root "$ROOTFS" \
        --initdb \
        add --no-cache \
        gnome-session gnome-shell gnome-settings-daemon \
        gsettings-desktop-schemas adwaita-icon-theme cantarell-fonts
fi

if [ -x "$SCRIPT_DIR/build-mutter.sh" ] && [ "${RUSTOS_BUILD_MUTTER:-0}" = "1" ]; then
    echo "=== Building Mutter into rootfs (RUSTOS_BUILD_MUTTER=1) ==="
    "$SCRIPT_DIR/build-mutter.sh"
fi

echo "=== Rebuilding initramfs ==="
"$SCRIPT_DIR/build_initramfs.sh" "$ROOTFS" "$PROJECT_DIR/userspace/initramfs.cpio"
echo "Done. Rebuild kernel: make bootimage"
