#!/bin/bash
# Install gnome-shell and session stack into userspace/rootfs from Alpine x86_64 packages.
#
# Works on macOS (arm64) and Linux — uses curl + APKINDEX dependency resolution,
# no host apk binary required.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
ROOTFS="$PROJECT_DIR/userspace/rootfs"
BUILD_DIR="$PROJECT_DIR/build/gnome-shell-build"
ALPINE_MIRROR="${ALPINE_MIRROR:-https://dl-cdn.alpinelinux.org/alpine}"
ALPINE_VERSION="${ALPINE_VERSION:-v3.21}"
ALPINE_ARCH="${ALPINE_ARCH:-x86_64}"

mkdir -p "$BUILD_DIR" "$ROOTFS/usr/bin" "$ROOTFS/usr/lib"

fetch_apk() {
    pkg=$1
    dest="$BUILD_DIR/${pkg}.apk"
    if [ -f "$dest" ] && [ -s "$dest" ]; then
        return 0
    fi
    for repo in main community; do
        URL="$ALPINE_MIRROR/$ALPINE_VERSION/$repo/$ALPINE_ARCH/"
        PKG_FILE=$(curl -fsSL "$URL" | grep -oE "href=\"${pkg}-[0-9][^\"]*\.apk\"" | head -1 | sed 's/href="//;s/"//')
        if [ -n "$PKG_FILE" ]; then
            echo "  fetch $PKG_FILE"
            curl -fsSL "$URL$PKG_FILE" -o "$dest"
            return 0
        fi
    done
    echo "  WARNING: package not found: $pkg" >&2
    return 1
}

extract_apk() {
    pkg=$1
    dest="$BUILD_DIR/${pkg}.apk"
    if [ ! -f "$dest" ]; then
        return 1
    fi
    tar xzf "$dest" -C "$ROOTFS" 2>/dev/null || true
}

echo "=== Resolving Alpine GNOME package dependencies ==="
PACKAGES=$(python3 "$SCRIPT_DIR/alpine-resolve-deps.py" \
    gnome-shell gnome-session gnome-settings-daemon \
    gsettings-desktop-schemas adwaita-icon-theme cantarell-fonts mutter \
    font-ubuntu gnome-themes-extra gnome-shell-extensions)
PKG_COUNT=$(printf '%s\n' "$PACKAGES" | sed '/^$/d' | wc -l | tr -d ' ')
echo "  ${PKG_COUNT} packages to install"

echo "=== Downloading and extracting into rootfs ==="
printf '%s\n' "$PACKAGES" | while IFS= read -r pkg; do
    [ -n "$pkg" ] || continue
    fetch_apk "$pkg" || true
    extract_apk "$pkg" || true
done

echo "=== Verifying gnome-shell ==="
if [ ! -e "$ROOTFS/usr/bin/gnome-shell" ]; then
    echo "ERROR: gnome-shell not installed — check network and Alpine mirror" >&2
    exit 1
fi

ls -la "$ROOTFS/usr/bin/gnome-shell"
file "$ROOTFS/usr/bin/gnome-shell" 2>/dev/null || true

for bin in gnome-session mutter; do
    if [ -e "$ROOTFS/usr/bin/$bin" ] || [ -L "$ROOTFS/usr/bin/$bin" ]; then
        echo "  ready: /usr/bin/$bin"
    fi
done

echo "=== Rebuilding initramfs ==="
"$SCRIPT_DIR/build_initramfs.sh" "$ROOTFS" "$PROJECT_DIR/userspace/initramfs.cpio"

echo ""
echo "=== gnome-shell install complete ==="
echo "  Binary: $ROOTFS/usr/bin/gnome-shell"
echo "  Next:   make bootimage && make run"
