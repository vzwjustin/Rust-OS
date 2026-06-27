#!/usr/bin/env bash
# Build glib-native as a static library on the host toolchain.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
HOST_TARGET="${HOST_TARGET:-$(rustc -vV | sed -n 's/^host: //p')}"

if [[ -z "${HOST_TARGET}" ]]; then
    echo "error: could not determine host target from rustc -vV" >&2
    exit 1
fi

cd "${ROOT}"
echo "[glib-native] building staticlib on host target: ${HOST_TARGET}"
cargo build -p glib-native --release --target "${HOST_TARGET}" --features c-abi

TARGET_DIR="$(cargo metadata --format-version=1 --no-deps | python3 -c "import sys,json; print(json.load(sys.stdin)['target_directory'])")"
STATICLIB="${TARGET_DIR}/${HOST_TARGET}/release/libglib_native.a"
if [[ ! -f "${STATICLIB}" ]]; then
    echo "error: expected static library not found at ${STATICLIB}" >&2
    exit 1
fi

echo "[glib-native] static library: ${STATICLIB}"
