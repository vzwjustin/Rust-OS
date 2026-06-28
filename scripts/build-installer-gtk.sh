#!/bin/bash
# Install GTK3 + zenity (graphical installer UI) into userspace/rootfs from Alpine.
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
ROOTFS="$PROJECT_DIR/userspace/rootfs"
BUILD_DIR="$PROJECT_DIR/build/installer-gtk-build"
ALPINE_MIRROR="${ALPINE_MIRROR:-https://dl-cdn.alpinelinux.org/alpine}"
ALPINE_VERSION="${ALPINE_VERSION:-v3.21}"
ALPINE_ARCH="${ALPINE_ARCH:-x86_64}"
mkdir -p "$BUILD_DIR" "$ROOTFS/usr/bin" "$ROOTFS/usr/lib"
fetch_apk() {
    pkg=$1
    dest="$BUILD_DIR/${pkg}.apk"
    if [ -f "$dest" ] && [ -s "$dest" ]; then return 0; fi
    for repo in main community; do
        URL="$ALPINE_MIRROR/$ALPINE_VERSION/$repo/$ALPINE_ARCH/"
        PKG_FILE=$(curl -fsSL "$URL" | APK_PKG="$pkg" python3 -c 'import os, re, sys
pkg = os.environ["APK_PKG"]
for href, label in re.findall(r"href=\"([^\"]*\.apk)\">([^<]*\.apk)<", sys.stdin.read()):
    if label.startswith(pkg + "-"):
        print(href)
        break')
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
    [ -f "$dest" ] || return 1
    tar xzf "$dest" -C "$ROOTFS" 2>/dev/null || true
}
echo "=== Resolving GTK installer package dependencies ==="
PACKAGES=$(python3 "$SCRIPT_DIR/alpine-resolve-deps.py" \
    zenity gtk4.0 libadwaita adwaita-icon-theme font-ubuntu \
    dbus at-spi2-core hicolor-icon-theme)
PKG_COUNT=$(printf '%s\n' "$PACKAGES" | sed '/^$/d' | wc -l | tr -d ' ')
echo "  ${PKG_COUNT} packages to install"
printf '%s\n' "$PACKAGES" | while IFS= read -r pkg; do
    [ -n "$pkg" ] || continue
    fetch_apk "$pkg" || true
    extract_apk "$pkg" || true
done
for bin in zenity; do
    [ -e "$ROOTFS/usr/bin/$bin" ] || { echo "ERROR: $bin missing" >&2; exit 1; }
done
ls -la "$ROOTFS/usr/bin/zenity"
echo "=== GTK installer stack complete ==="
