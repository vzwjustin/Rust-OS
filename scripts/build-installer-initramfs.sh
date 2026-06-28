#!/bin/bash
# Build a minimal installer initramfs (~30 MB target) from userspace/installer-rootfs.
#
# Populates userspace/installer-rootfs/ with busybox, installer init, and optional
# tools copied from userspace/rootfs, then packs userspace/installer-initramfs.cpio.
#
# Optional Alpine packages for full disk tooling (install on host with apk --root):
#   mount      - busybox (default)
#   e2fsprogs  - mkfs.ext4, fsck.ext4
#   dosfstools - mkfs.vfat
#
# Usage:
#   ./scripts/build-installer-initramfs.sh [--rootfs-only] [ROOTFS] [OUTPUT]

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"

ROOTFS_ONLY=0
ARGS=()
for arg in "$@"; do
    case "$arg" in
        --rootfs-only)
            ROOTFS_ONLY=1
            ;;
        *)
            ARGS+=("$arg")
            ;;
    esac
done

INSTALLER_ROOTFS="${ARGS[0]:-$PROJECT_DIR/userspace/installer-rootfs}"
OUTPUT="${ARGS[1]:-$PROJECT_DIR/userspace/installer-initramfs.cpio}"
SOURCE_ROOTFS="$PROJECT_DIR/userspace/rootfs"
INIT_SRC="$INSTALLER_ROOTFS/usr/bin/rustos-installer-init"

log() {
    echo "[installer-initramfs] $*"
}

warn() {
    echo "[installer-initramfs] WARNING: $*" >&2
}

copy_or_link_busybox_tool() {
    local name=$1
    local dest="$INSTALLER_ROOTFS/sbin/$name"

    if [ -e "$SOURCE_ROOTFS/sbin/$name" ]; then
        cp -a "$SOURCE_ROOTFS/sbin/$name" "$dest"
        return
    fi
    if [ -e "$SOURCE_ROOTFS/bin/$name" ]; then
        cp -a "$SOURCE_ROOTFS/bin/$name" "$dest"
        return
    fi

    ln -sf ../bin/busybox "$dest"
}

setup_installer_rootfs() {
    log "Setting up installer rootfs at $INSTALLER_ROOTFS"

    mkdir -p \
        "$INSTALLER_ROOTFS/bin" \
        "$INSTALLER_ROOTFS/sbin" \
        "$INSTALLER_ROOTFS/usr/bin" \
        "$INSTALLER_ROOTFS/etc/rustos" \
        "$INSTALLER_ROOTFS/dev" \
        "$INSTALLER_ROOTFS/proc" \
        "$INSTALLER_ROOTFS/sys" \
        "$INSTALLER_ROOTFS/run" \
        "$INSTALLER_ROOTFS/tmp"

    if [ ! -f "$INIT_SRC" ]; then
        echo "Error: missing $INIT_SRC" >&2
        exit 1
    fi
    chmod +x "$INIT_SRC"

    if [ -f "$SOURCE_ROOTFS/bin/busybox" ]; then
        cp -a "$SOURCE_ROOTFS/bin/busybox" "$INSTALLER_ROOTFS/bin/busybox"
    else
        echo "Error: busybox not found at $SOURCE_ROOTFS/bin/busybox" >&2
        exit 1
    fi

    ln -sf busybox "$INSTALLER_ROOTFS/bin/sh"
    ln -sf /usr/bin/rustos-installer-init "$INSTALLER_ROOTFS/init"

    copy_or_link_busybox_tool mount
    copy_or_link_busybox_tool mkfs.vfat

    if [ -e "$SOURCE_ROOTFS/sbin/mkfs.ext4" ]; then
        cp -a "$SOURCE_ROOTFS/sbin/mkfs.ext4" "$INSTALLER_ROOTFS/sbin/mkfs.ext4"
    else
        ln -sf ../bin/busybox "$INSTALLER_ROOTFS/sbin/mkfs.ext4"
        warn "mkfs.ext4 is busybox symlink; install e2fsprogs in rootfs for full ext4 support"
    fi

    for bin in rustos-installer rustos-installer-ui rustos-installer-postinstall rustos-live-session rustos-installer-gtk-session; do
        if [ -f "$SOURCE_ROOTFS/usr/bin/$bin" ]; then
            cp -a "$SOURCE_ROOTFS/usr/bin/$bin" "$INSTALLER_ROOTFS/usr/bin/$bin"
            chmod +x "$INSTALLER_ROOTFS/usr/bin/$bin"
            log "Copied $bin from rootfs"
        else
            warn "$bin not found in rootfs (skipped)"
        fi
    done

    if [ -f "$SOURCE_ROOTFS/etc/rustos/installer.conf" ]; then
        cp -a "$SOURCE_ROOTFS/etc/rustos/installer.conf" "$INSTALLER_ROOTFS/etc/rustos/installer.conf"
    fi

  # Copy shared libraries required by any copied installer binaries.
    if command -v ldd >/dev/null 2>&1; then
        for bin in "$INSTALLER_ROOTFS/usr/bin/"rustos-installer*; do
            [ -f "$bin" ] || continue
            if file "$bin" 2>/dev/null | grep -q ELF; then
                while IFS= read -r lib; do
                    case "$lib" in
                        /lib/*|/usr/lib/*)
                            dest="$INSTALLER_ROOTFS$lib"
                            mkdir -p "$(dirname "$dest")"
                            if [ -f "$SOURCE_ROOTFS$lib" ]; then
                                cp -a "$SOURCE_ROOTFS$lib" "$dest"
                            fi
                            ;;
                    esac
                done < <(ldd "$bin" 2>/dev/null | awk '/=> \// {print $3} /^\// {print $1}')
            fi
        done
    fi

    local rootfs_bytes
    rootfs_bytes=$(du -sk "$INSTALLER_ROOTFS" | awk '{print $1}')
    local rootfs_mb=$((rootfs_bytes / 1024))
    log "Installer rootfs size: ${rootfs_mb} MB (${rootfs_bytes} KB)"
    if [ "$rootfs_mb" -gt 35 ]; then
        warn "Installer rootfs exceeds ~30 MB target (${rootfs_mb} MB)"
    fi
}

pack_initramfs() {
    if ! command -v cpio >/dev/null 2>&1; then
        echo "Error: cpio not found. Install cpio (e.g. apt-get install cpio)." >&2
        exit 1
    fi

    mkdir -p "$(dirname "$OUTPUT")"
    (
        cd "$INSTALLER_ROOTFS"
        find . -print0 | cpio --null --create --verbose --format=newc
    ) > "$OUTPUT"

  local bytes
    bytes=$(wc -c < "$OUTPUT" | tr -d ' ')
    local mb=$((bytes / 1024 / 1024))
    log "Built initramfs: $OUTPUT (${mb} MB, ${bytes} bytes)"
}

setup_installer_rootfs

if [ "$ROOTFS_ONLY" -eq 0 ]; then
    pack_initramfs
else
    log "Rootfs-only mode; skipping cpio pack"
fi
