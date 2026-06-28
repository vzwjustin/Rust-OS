#!/bin/bash
# Build Mutter as a userspace binary for RustOS.
#
# This script:
# 1. Downloads Alpine's apk-static for x86_64 to fetch dependency packages
# 2. Creates a sysroot with all Mutter build dependencies
# 3. Cross-compiles Mutter with meson (minimal features, no GL/EGL/X11)
# 4. Installs the binary and shared libs into the rootfs
#
# Requirements (macOS): brew install meson ninja pkg-config llvm
# The Alpine apk-static binary runs under Rosetta on arm64 macOS.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
MUTTER_SRC="$PROJECT_DIR/mutter"
BUILD_DIR="$PROJECT_DIR/build/mutter-build"
SYSROOT="$PROJECT_DIR/build/mutter-sysroot"
ROOTFS="$PROJECT_DIR/userspace/rootfs"
ALPINE_MIRROR="https://dl-cdn.alpinelinux.org/alpine"
ALPINE_VERSION="edge"
ALPINE_ARCH="x86_64"

# --- Phase 1: Download Alpine packages directly (curl, no apk-static needed) ---

echo "=== Phase 1: Downloading Alpine x86_64 packages ==="
mkdir -p "$BUILD_DIR" "$SYSROOT"

# Packages needed for Mutter (minimal build, no GL/EGL/X11)
# Each package is downloaded as an .apk (gzip tar) and extracted into the sysroot
PACKAGES="
    alpine-baselayout
    alpine-baselayout-data
    musl
    musl-dev
    linux-headers
    glib
    glib-dev
    glib-static
    wayland-libs-client
    wayland-libs-server
    wayland-dev
    wayland-protocols
    cairo
    cairo-dev
    pango
    pango-dev
    pixman
    pixman-dev
    graphene
    graphene-dev
    libxml2
    libxml2-dev
    fribidi
    fribidi-dev
    harfbuzz
    harfbuzz-dev
    fontconfig
    fontconfig-dev
    freetype
    freetype-dev
    json-glib
    json-glib-dev
    libdrm
    libdrm-dev
    libinput
    libinput-dev
    libxkbcommon
    libxkbcommon-dev
    mtdev
    mtdev-dev
    eudev-libs
    eudev-dev
    dbus-libs
    dbus-dev
    gsettings-desktop-schemas
    libffi
    libffi-dev
    pcre2
    pcre2-dev
    gettext
    gettext-dev
    zlib
    zlib-dev
    bzip2
    bzip2-dev
    libpng
    libpng-dev
    expat
    expat-dev
    util-linux
    util-linux-dev
    libxau
    libxau-dev
    libxcb
    libxcb-dev
    libxdmcp
    libxdmcp-dev
    xorgproto
    libsm
    libsm-dev
    libice
    libice-dev
    libxtst
    libxtst-dev
    libxi
    libxi-dev
    shared-mime-info
    xz
    xz-dev
    liblzma
    brotli
    brotli-dev
    libx11
    libx11-dev
    libxext
    libxext-dev
    libxrender
    libxrender-dev
    libxfixes
    libxfixes-dev
    libxrandr
    libxrandr-dev
    libxcomposite
    libxcomposite-dev
    libxcursor
    libxcursor-dev
    libxdamage
    libxdamage-dev
    libxinerama
    libxinerama-dev
    libxshmfence
    libxshmfence-dev
    libxscrnsaver
    libxscrnsaver-dev
    libxxf86vm
    libxxf86vm-dev
    libxv
    libxv-dev
    libxi-dev
    libxtst-dev
    xcb-util
    xcb-util-dev
    xcb-util-image
    xcb-util-image-dev
    xcb-util-wm
    xcb-util-wm-dev
    xcb-util-keysyms
    xcb-util-keysyms-dev
    xcb-util-renderutil
    xcb-util-renderutil-dev
    libxkbfile
    libxkbfile-dev
    atk
    atk-dev
    at-spi2-core
    at-spi2-core-dev
    colord
    colord-dev
    lcms2
    lcms2-dev
    libei
    libei-dev
    libdisplay-info
    libdisplay-info-dev
    libdrm
    libdrm-dev
    mesa
    mesa-dev
    mesa-gles
    mesa-egl
    mesa-gl
    mesa-glapi
    elogind
    elogind-dev
    libgudev
    libgudev-dev
"

fetch_package() {
    pkg=$1
    for repo in main community; do
        URL="$ALPINE_MIRROR/$ALPINE_VERSION/$repo/$ALPINE_ARCH/"
        # List directory and find matching package
        PKG_FILE=$(curl -fsSL "$URL" | APK_PKG="$pkg" python3 -c 'import os, re, sys
pkg = os.environ["APK_PKG"]
for href, label in re.findall(r"href=\"([^\"]*\.apk)\">([^<]*\.apk)<", sys.stdin.read()):
    if label.startswith(pkg + "-"):
        print(href)
        break')
        if [ -n "$PKG_FILE" ]; then
            echo "  Downloading $PKG_FILE..."
            curl -sL "$URL$PKG_FILE" -o "$BUILD_DIR/${pkg}.apk"
            tar xzf "$BUILD_DIR/${pkg}.apk" -C "$SYSROOT" 2>/dev/null || true
            return 0
        fi
    done
    echo "  WARNING: Could not find package $pkg"
    return 1
}

for pkg in $PACKAGES; do
    fetch_package "$pkg" || true
done

echo "  Sysroot populated"

# Create stub GCC compatibility files (musl doesn't need them, but clang looks for them)
echo "  Creating GCC compatibility stubs..."
LLVM_BIN="/opt/homebrew/opt/llvm/bin"
"$LLVM_BIN/llvm-ar" rcs "$SYSROOT/usr/lib/libgcc.a" 2>/dev/null || true
"$LLVM_BIN/llvm-ar" rcs "$SYSROOT/usr/lib/libgcc_s.a" 2>/dev/null || true
"$LLVM_BIN/llvm-ar" rcs "$SYSROOT/usr/lib/libintl.a" 2>/dev/null || true
rm -f "$SYSROOT/usr/lib/libintl.so"
ln -s libintl.a "$SYSROOT/usr/lib/libintl.so"
echo '' | clang -target x86_64-alpine-linux-musl -c -x c - -o "$SYSROOT/usr/lib/crtbeginS.o" 2>/dev/null || true
echo '' | clang -target x86_64-alpine-linux-musl -c -x c - -o "$SYSROOT/usr/lib/crtendS.o" 2>/dev/null || true
echo '' | clang -target x86_64-alpine-linux-musl -c -x c - -o "$SYSROOT/usr/lib/crtbegin.o" 2>/dev/null || true
echo '' | clang -target x86_64-alpine-linux-musl -c -x c - -o "$SYSROOT/usr/lib/crtend.o" 2>/dev/null || true

# Create pkg-config stubs for packages not in Alpine repos
mkdir -p "$SYSROOT/usr/lib/pkgconfig"
cat > "$SYSROOT/usr/lib/pkgconfig/glycin-2.pc" << 'GLYCIN_EOF'
prefix=/usr
exec_prefix=${prefix}
libdir=${exec_prefix}/lib
includedir=${prefix}/include

Name: glycin-2
Description: Glycin image loading (stub for cross-compile)
Version: 2.0.0
Libs: -L${libdir}
Cflags: -I${includedir}
GLYCIN_EOF

cat > "$SYSROOT/usr/lib/pkgconfig/gsettings-desktop-schemas.pc" << 'GDS_EOF'
prefix=/usr
exec_prefix=${prefix}
libdir=${exec_prefix}/lib
includedir=${prefix}/include

Name: gsettings-desktop-schemas
Description: GSettings desktop schemas (stub for cross-compile)
Version: 47.1
Cflags: -I${includedir}
GDS_EOF

# libei-1.0 and libeis-1.0 stubs (Alpine 3.21 has 1.3.0, Mutter needs >= 1.3.901)
cat > "$SYSROOT/usr/lib/pkgconfig/libeis-1.0.pc" << 'LIBEIS_EOF'
prefix=/usr
exec_prefix=${prefix}
libdir=${exec_prefix}/lib
includedir=${prefix}/include

Name: libeis-1.0
Description: libeis (stub for cross-compile)
Version: 1.3.901
Libs: -L${libdir} -leis-1.0
Cflags: -I${includedir}
LIBEIS_EOF

cat > "$SYSROOT/usr/lib/pkgconfig/libei-1.0.pc" << 'LIBEI_EOF'
prefix=/usr
exec_prefix=${prefix}
libdir=${exec_prefix}/lib
includedir=${prefix}/include

Name: libei-1.0
Description: libei (stub for cross-compile)
Version: 1.3.901
Libs: -L${libdir} -lei-1.0
Cflags: -I${includedir}
LIBEI_EOF

# --- Phase 3: Set up cross-compilation ---

echo "=== Phase 3: Configuring cross-compilation ==="

# Create a cross pkg-config wrapper
cat > "$BUILD_DIR/cross-pkg-config" << EOF
#!/bin/bash
export PKG_CONFIG_LIBDIR="$SYSROOT/usr/lib/pkgconfig:$SYSROOT/usr/share/pkgconfig"
export PKG_CONFIG_SYSROOT_DIR="$SYSROOT"
exec pkg-config "\$@"
EOF
chmod +x "$BUILD_DIR/cross-pkg-config"

# Create meson cross file
LLVM_BIN="/opt/homebrew/opt/llvm/bin"
cat > "$BUILD_DIR/mutter-cross.txt" << EOF
[binaries]
c = 'clang'
cpp = 'clang++'
ar = '$LLVM_BIN/llvm-ar'
strip = '$LLVM_BIN/llvm-strip'
ld = 'ld.lld'
pkg-config = '$BUILD_DIR/cross-pkg-config'

[built-in options]
c_args = ['-target', 'x86_64-alpine-linux-musl', '--sysroot=$SYSROOT', '-I$SYSROOT/usr/include', '-D__MUSL__']
c_link_args = ['-target', 'x86_64-alpine-linux-musl', '--sysroot=$SYSROOT', '-fuse-ld=lld', '-L$SYSROOT/usr/lib', '-L$SYSROOT/lib', '-Wl,-rpath,/usr/lib', '-Wl,--dynamic-linker,/lib/ld-musl-x86_64.so.1']
cpp_args = ['-target', 'x86_64-alpine-linux-musl', '--sysroot=$SYSROOT', '-I$SYSROOT/usr/include', '-D__MUSL__']
cpp_link_args = ['-target', 'x86_64-alpine-linux-musl', '--sysroot=$SYSROOT', '-fuse-ld=lld', '-L$SYSROOT/usr/lib', '-L$SYSROOT/lib', '-Wl,-rpath,/usr/lib', '-Wl,--dynamic-linker,/lib/ld-musl-x86_64.so.1']

[host_machine]
system = 'linux'
cpu_family = 'x86_64'
cpu = 'x86_64'
endian = 'little'
EOF

# --- Phase 4: Configure and build Mutter ---

echo "=== Phase 4: Configuring Mutter (minimal, no GL/EGL/X11) ==="
cd "$MUTTER_SRC"

# Clean any previous build
rm -rf build-rustos

meson setup build-rustos \
    --cross-file "$BUILD_DIR/mutter-cross.txt" \
    -Dopengl=false \
    -Dgles2=true \
    -Degl=true \
    -Dxwayland=false \
    -Dremote_desktop=false \
    -Dlibgnome_desktop=false \
    -Dlibwacom=false \
    -Dsound_player=false \
    -Dstartup_notification=false \
    -Dintrospection=false \
    -Ddocs=false \
    -Dprofiler=false \
    -Dfonts=false \
    -Dbash_completion=false \
    -Dtests=disabled \
    -Dcogl_tests=false \
    -Dclutter_tests=false \
    -Dmutter_tests=false \
    -Dverbose=true \
    --default-library=shared \
    --prefix=/usr \
    -Dpkg_config_path=$SYSROOT/usr/lib/pkgconfig:$SYSROOT/usr/share/pkgconfig \
    2>&1 || {
        echo "ERROR: meson setup failed. Check dependencies in sysroot."
        exit 1
    }

echo "=== Phase 5: Building Mutter ==="
JOBS=$(sysctl -n hw.ncpu 2>/dev/null || echo 4)
ninja -C build-rustos -j"$JOBS" 2>&1 || {
    echo "ERROR: ninja build failed."
    exit 1
}

echo "=== Phase 6: Installing Mutter to sysroot and rootfs ==="
DESTDIR="$SYSROOT" ninja -C build-rustos install 2>&1 || true

# Copy mutter binary to rootfs
mkdir -p "$ROOTFS/usr/bin" "$ROOTFS/usr/lib"

if [ -f "$SYSROOT/usr/bin/mutter" ]; then
    cp "$SYSROOT/usr/bin/mutter" "$ROOTFS/usr/bin/mutter"
    echo "  Copied mutter binary to rootfs"
else
    echo "  WARNING: mutter binary not found"
fi

# Copy all shared libraries to rootfs
echo "  Copying shared libraries..."
for lib in "$SYSROOT/usr/lib/"*.so*; do
    [ -f "$lib" ] || continue
    cp -L "$lib" "$ROOTFS/usr/lib/" 2>/dev/null || true
done

# Copy glib schemas
mkdir -p "$ROOTFS/usr/share/glib-2.0/schemas"
cp -r "$SYSROOT/usr/share/glib-2.0/schemas/"* "$ROOTFS/usr/share/glib-2.0/schemas/" 2>/dev/null || true

# Copy wayland protocols
mkdir -p "$ROOTFS/usr/share/wayland-protocols"
cp -r "$SYSROOT/usr/share/wayland-protocols/"* "$ROOTFS/usr/share/wayland-protocols/" 2>/dev/null || true

echo ""
echo "=== Mutter build complete ==="
echo "Binary: $ROOTFS/usr/bin/mutter"
echo "Libs:   $ROOTFS/usr/lib/"
echo ""
echo "To rebuild the initramfs:"
echo "  cd $PROJECT_DIR && make bootimage"
