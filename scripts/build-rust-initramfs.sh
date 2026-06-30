#!/usr/bin/env bash
# Build the freestanding Rust PID1 and overlay it into the initramfs.
set -euo pipefail

PROJECT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BASE_INITRAMFS_GZ="$PROJECT_DIR/userspace/initramfs.cpio.gz"
OUT_CPIO="$PROJECT_DIR/userspace/initramfs.cpio"
OUT_GZ="$PROJECT_DIR/userspace/initramfs.cpio.gz"
ROOTFS_DIR="$PROJECT_DIR/build/rust-init-rootfs"
TARGET_DIR="$PROJECT_DIR/target/userspace"
LINKER_SCRIPT="$PROJECT_DIR/userspace/rust-init/linker.ld"

if ! command -v cpio >/dev/null 2>&1; then
  echo "cpio is required to rebuild the initramfs" >&2
  exit 1
fi

export CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-12}"
export RUSTC_WRAPPER="${RUSTC_WRAPPER:-}"
export CARGO_BUILD_RUSTC_WRAPPER="${CARGO_BUILD_RUSTC_WRAPPER:-}"

rm -f "$TARGET_DIR/x86_64-rustos/release/rustos-init"

cargo rustc \
  --manifest-path "$PROJECT_DIR/userspace/rust-init/Cargo.toml" \
  --target "$PROJECT_DIR/x86_64-rustos.json" \
  --target-dir "$TARGET_DIR" \
  -Zjson-target-spec \
  -Zbuild-std=core,compiler_builtins \
  --release \
  -- \
  -C "link-arg=-T$LINKER_SCRIPT" \
  -C relocation-model=static

INIT_ELF="$TARGET_DIR/x86_64-rustos/release/rustos-init"
rm -rf "$ROOTFS_DIR"
mkdir -p "$ROOTFS_DIR"

(
  cd "$ROOTFS_DIR"
  gzip -dc "$BASE_INITRAMFS_GZ" | cpio -id --quiet --no-absolute-filenames
  mkdir -p bin sbin proc sys dev run tmp
  rm -f init bin/init sbin/init
  install -m 0755 "$INIT_ELF" init
  install -m 0755 "$INIT_ELF" bin/init
  install -m 0755 "$INIT_ELF" sbin/init
  find . -print0 | cpio --null -o --quiet --format=newc > "$OUT_CPIO"
)

gzip -9 -c "$OUT_CPIO" > "$OUT_GZ"
echo "wrote $OUT_CPIO and $OUT_GZ with $INIT_ELF as PID1"
