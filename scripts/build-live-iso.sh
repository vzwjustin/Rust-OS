#!/bin/bash
# Build RustOS live/install ISO: kernel + installer initramfs + live squashfs.
#
# Layout:
#   build/iso/boot/rustos
#   build/iso/boot/initrd.img
#   build/iso/live/filesystem.squashfs
#   build/iso/.disk/info
#
# Uses xorriso or grub-mkrescue when available; otherwise stages files and prints
# manual QEMU boot instructions.
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

    cp -a "$BOOTIMAGE" "$ISO_ROOT/boot/rustos"
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

write_grub_cfg() {
    mkdir -p "$ISO_ROOT/boot/grub"
    cat > "$ISO_ROOT/boot/grub/grub.cfg" <<'EOF'
set timeout=5
set default=0

menuentry "RustOS Live (try)" {
    linux /boot/rustos rustos.boot=live
    initrd /boot/initrd.img
}

menuentry "RustOS Install" {
    linux /boot/rustos rustos.boot=install
    initrd /boot/initrd.img
}
EOF
}

build_iso_xorriso() {
    write_grub_cfg
    log "Creating ISO with xorriso..."
    xorriso -as mkisofs \
        -iso-level 3 \
        -full-iso9660-filenames \
        -volid "RUSTOS_LIVE" \
        -eltorito-boot boot/grub/grub.img \
        -no-emul-boot \
        -boot-load-size 4 \
        -boot-info-table \
        --grub2-boot-info \
        --grub2-mbr /usr/lib/grub/i386-pc/boot_hybrid.img \
        -eltorito-alt-boot \
        -e EFI/efiboot.img \
        -no-emul-boot \
        -append_partition 2 0xef "$(dirname "$(command -v grub-mkrescue)")/../share/grub/i386-pc/eltorito.img" 2>/dev/null || true \
        -output "$ISO_OUT" \
        "$ISO_ROOT" 2>/dev/null || {
            xorriso -as mkisofs \
                -iso-level 3 \
                -full-iso9660-filenames \
                -volid "RUSTOS_LIVE" \
                -output "$ISO_OUT" \
                "$ISO_ROOT"
        }
    log "ISO written: $ISO_OUT ($(du -h "$ISO_OUT" | awk '{print $1}'))"
}

build_iso_grub_mkrescue() {
    write_grub_cfg
    log "Creating ISO with grub-mkrescue..."
    grub-mkrescue -o "$ISO_OUT" "$ISO_ROOT" -- \
        -volid RUSTOS_LIVE
    log "ISO written: $ISO_OUT ($(du -h "$ISO_OUT" | awk '{print $1}'))"
}

print_manual_qemu() {
    local live_image="$ISO_ROOT/live/filesystem.squashfs"
    if [ ! -f "$live_image" ] && [ -f "$ISO_ROOT/live/filesystem.tar.gz" ]; then
        live_image="$ISO_ROOT/live/filesystem.tar.gz"
    fi

    cat <<EOF

[live-iso] ISO tooling not found (install xorriso and grub-pc/grub-efi).
Staged tree: $ISO_ROOT

Manual QEMU (kernel + initrd + live root via virtio-blk):

  qemu-system-x86_64 \\
    -kernel $ISO_ROOT/boot/rustos \\
    -initrd $ISO_ROOT/boot/initrd.img \\
    -append "rustos.boot=install" \\
    -m 2G -smp 2 \\
    -serial stdio \\
    -display cocoa \\
    -device virtio-blk,drive=live \\
    -drive id=live,file=$live_image,format=raw,if=none,readonly=on

Try live session:

  -append "rustos.boot=live"

EOF
}

ensure_prerequisites
stage_iso_tree

if command -v grub-mkrescue >/dev/null 2>&1; then
    build_iso_grub_mkrescue
elif command -v xorriso >/dev/null 2>&1; then
    build_iso_xorriso
else
    print_manual_qemu
fi

log "ISO contents:"
du -sh "$ISO_ROOT"/* 2>/dev/null || true
