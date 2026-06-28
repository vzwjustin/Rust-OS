#!/bin/bash
# Build RustOS live/install media: bootimage + installer initramfs + live squashfs.
#
# Layout:
#   build/iso/boot/bootimage-rustos.bin
#   build/iso/boot/initrd.img
#   build/iso/live/filesystem.squashfs
#   build/iso/.disk/info
#
# Uses xorriso when available to wrap the bootloader raw disk image; otherwise
# stages files and prints manual QEMU boot instructions.
#
# Usage:
#   ./scripts/build-live-iso.sh [--release]

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"

RELEASE=0
for arg in "$@"; do
    case "$arg" in
        --release)
            RELEASE=1
            ;;
    esac
done

ISO_ROOT="$PROJECT_DIR/build/iso"
ISO_OUT="$PROJECT_DIR/build/rustos-live.iso"
PROFILE_DIR=debug
if [ "$RELEASE" -eq 1 ]; then
    PROFILE_DIR=release
fi

BOOTIMAGE_CANDIDATES=(
    "$PROJECT_DIR/target/x86_64-rustos/$PROFILE_DIR/bootimage-rustos.bin"
    "$PROJECT_DIR/target/x86_64-rustos.json/$PROFILE_DIR/bootimage-rustos.bin"
    "$PROJECT_DIR/target/x86_64-rustos/$PROFILE_DIR/kernel-x86_64-bios"
    "$PROJECT_DIR/target/x86_64-rustos.json/$PROFILE_DIR/kernel-x86_64-bios"
)

log() {
    echo "[live-iso] $*"
}

find_bootimage() {
    local candidate
    for candidate in "${BOOTIMAGE_CANDIDATES[@]}"; do
        if [ -f "$candidate" ]; then
            echo "$candidate"
            return 0
        fi
    done
    return 1
}

ensure_prerequisites() {
    log "Building installer initramfs..."
    "$SCRIPT_DIR/build-installer-initramfs.sh"

    if [ ! -f "$PROJECT_DIR/userspace/installer-initramfs.cpio" ]; then
        echo "Error: installer initramfs not found" >&2
        exit 1
    fi

    log "Building live squashfs..."
    "$SCRIPT_DIR/build-live-squashfs.sh"

    if [ ! -f "$PROJECT_DIR/build/live/filesystem.squashfs" ] && \
       [ ! -f "$PROJECT_DIR/build/live/filesystem.tar.gz" ]; then
        echo "Error: live filesystem image not found" >&2
        exit 1
    fi

    local bootimage
    if ! bootimage=$(find_bootimage); then
        log "Bootimage missing; building with make bootimage$([ "$RELEASE" -eq 1 ] && echo '-release' || echo '')..."
        if [ "$RELEASE" -eq 1 ]; then
            make -C "$PROJECT_DIR" bootimage-release
        else
            make -C "$PROJECT_DIR" bootimage
        fi
        bootimage=$(find_bootimage) || {
            echo "Error: bootimage not found after build" >&2
            exit 1
        }
    fi
    BOOTIMAGE="$bootimage"
    log "Using kernel image: $BOOTIMAGE"
}

stage_iso_tree() {
    log "Staging ISO tree at $ISO_ROOT"
    rm -rf "$ISO_ROOT"
    mkdir -p "$ISO_ROOT/boot" "$ISO_ROOT/live" "$ISO_ROOT/.disk"

    cp -a "$BOOTIMAGE" "$ISO_ROOT/boot/bootimage-rustos.bin"
    cp -a "$PROJECT_DIR/userspace/installer-initramfs.cpio" "$ISO_ROOT/boot/initrd.img"

    if [ -f "$PROJECT_DIR/build/live/filesystem.squashfs" ]; then
        cp -a "$PROJECT_DIR/build/live/filesystem.squashfs" "$ISO_ROOT/live/filesystem.squashfs"
    elif [ -f "$PROJECT_DIR/build/live/filesystem.tar.gz" ]; then
        cp -a "$PROJECT_DIR/build/live/filesystem.tar.gz" "$ISO_ROOT/live/filesystem.tar.gz"
        log "WARNING: ISO contains tar.gz fallback instead of squashfs"
    fi

    cat > "$ISO_ROOT/.disk/info" <<EOF
RustOS live/install media
Kernel: $(basename "$BOOTIMAGE")
Profile: $PROFILE_DIR
Built: $(date -u +"%Y-%m-%dT%H:%M:%SZ")
EOF
}

build_iso_xorriso() {
    log "Creating ISO with xorriso hard-disk boot emulation..."
    xorriso -as mkisofs \
        -iso-level 3 \
        -full-iso9660-filenames \
        -volid "RUSTOS_LIVE" \
        -b boot/bootimage-rustos.bin \
        -hard-disk-boot \
        -output "$ISO_OUT" \
        "$ISO_ROOT"
    log "ISO written: $ISO_OUT ($(du -h "$ISO_OUT" | awk '{print $1}'))"
}

print_manual_qemu() {
    local boot_image="$ISO_ROOT/boot/bootimage-rustos.bin"
    local live_image="$ISO_ROOT/live/filesystem.squashfs"
    if [ ! -f "$live_image" ] && [ -f "$ISO_ROOT/live/filesystem.tar.gz" ]; then
        live_image="$ISO_ROOT/live/filesystem.tar.gz"
    fi

    cat <<EOF

[live-iso] ISO tooling not found (install xorriso).
Staged tree: $ISO_ROOT

Manual QEMU (bootloader bootimage):

  qemu-system-x86_64 \
    -drive format=raw,file=$boot_image \
    -m 2G -smp 2 \
    -serial stdio \
    -display cocoa

Boot menu selection happens inside RustOS. Staged live payload:
  $live_image

EOF
}

ensure_prerequisites
stage_iso_tree

if command -v xorriso >/dev/null 2>&1; then
    build_iso_xorriso
else
    print_manual_qemu
fi

log "ISO contents:"
du -sh "$ISO_ROOT"/* 2>/dev/null || true
