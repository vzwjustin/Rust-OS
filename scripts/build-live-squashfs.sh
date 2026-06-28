#!/bin/bash
# Create a compressed live filesystem image from userspace/rootfs.
#
# Excludes Alpine package metadata (.PKGINFO, .SIGN.*, apk hooks) and writes:
#   build/live/filesystem.squashfs  (preferred, requires mksquashfs)
#   build/live/filesystem.tar.gz    (fallback when mksquashfs is unavailable)
#
# Usage:
#   ./scripts/build-live-squashfs.sh [ROOTFS] [OUTPUT_DIR]

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"

ROOTFS="${1:-$PROJECT_DIR/userspace/rootfs}"
OUTPUT_DIR="${2:-$PROJECT_DIR/build/live}"

SQUASHFS_OUT="$OUTPUT_DIR/filesystem.squashfs"
TARBALL_OUT="$OUTPUT_DIR/filesystem.tar.gz"
STAGING_DIR="$OUTPUT_DIR/rootfs-staging"

cleanup_staging() {
    if [ -d "$STAGING_DIR" ]; then
        chmod -R u+w "$STAGING_DIR" 2>/dev/null || true
        rm -rf "$STAGING_DIR"
    fi
}

log() {
    echo "[live-squashfs] $*"
}

human_size() {
    local bytes=$1
    if command -v numfmt >/dev/null 2>&1; then
        numfmt --to=iec-i --suffix=B "$bytes"
    else
        echo "${bytes} bytes"
    fi
}

report_size() {
    local path=$1
  local label=$2
    if [ -f "$path" ]; then
        local bytes
        bytes=$(wc -c < "$path" | tr -d ' ')
        log "$label: $path ($(human_size "$bytes"))"
    fi
}

if [ ! -d "$ROOTFS" ]; then
    echo "Error: rootfs directory not found at $ROOTFS" >&2
    exit 1
fi

mkdir -p "$OUTPUT_DIR"

log "Staging rootfs (excluding apk metadata)..."
cleanup_staging
mkdir -p "$STAGING_DIR"

rsync -a \
    --exclude='.PKGINFO' \
    --exclude='.SIGN.*' \
    --exclude='.pre-install' \
    --exclude='.pre-deinstall' \
    --exclude='.pre-upgrade' \
    --exclude='.post-install' \
    --exclude='.post-deinstall' \
    --exclude='.post-upgrade' \
    --exclude='.trigger' \
  "$ROOTFS"/ "$STAGING_DIR"/

staging_bytes=$(du -sk "$STAGING_DIR" | awk '{print $1}')
log "Staged rootfs: $((staging_bytes / 1024)) MB (${staging_bytes} KB uncompressed)"

if command -v mksquashfs >/dev/null 2>&1; then
    log "Creating squashfs with mksquashfs..."
    rm -f "$SQUASHFS_OUT"
    mksquashfs "$STAGING_DIR" "$SQUASHFS_OUT" \
        -comp zstd \
        -Xcompression-level 15 \
        -noappend \
        -e 'var/cache/*' 'var/tmp/*' 'tmp/*'
    report_size "$SQUASHFS_OUT" "Squashfs image"
    cleanup_staging
else
    warn_msg="mksquashfs not found; creating tar.gz fallback at $TARBALL_OUT"
    echo "[live-squashfs] WARNING: $warn_msg" >&2
    rm -f "$TARBALL_OUT"
    tar -C "$STAGING_DIR" -czf "$TARBALL_OUT" .
    report_size "$TARBALL_OUT" "Tarball fallback"
    cleanup_staging
    log "Install squashfs-tools for filesystem.squashfs output"
fi

log "Size report:"
report_size "$SQUASHFS_OUT" "  squashfs"
report_size "$TARBALL_OUT" "  tarball"
