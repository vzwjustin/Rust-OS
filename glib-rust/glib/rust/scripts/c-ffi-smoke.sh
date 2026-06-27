#!/usr/bin/env bash
# Link a minimal C program against libglib_native.a (host smoke test).
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
HOST_TARGET="${HOST_TARGET:-$(rustc -vV | sed -n 's/^host: //p')}"
TARGET_DIR="$(cd "${ROOT}" && cargo metadata --format-version=1 --no-deps | python3 -c "import sys,json; print(json.load(sys.stdin)['target_directory'])")"
BUILD_DIR="${TARGET_DIR}/${HOST_TARGET}/release"
STATICLIB="${BUILD_DIR}/libglib_native.a"
HEADER="${ROOT}/glib-native/include/glib_native.h"
SRC="${ROOT}/examples/c_ffi_smoke.c"
OUT="${BUILD_DIR}/c_ffi_smoke"

if [[ -z "${HOST_TARGET}" ]]; then
    echo "error: could not determine host target from rustc -vV" >&2
    exit 1
fi

cd "${ROOT}"

if [[ ! -f "${STATICLIB}" ]]; then
    echo "[c-ffi-smoke] static library missing; building..."
    ./scripts/build-static.sh
fi

if [[ ! -f "${STATICLIB}" ]]; then
    echo "error: static library not available at ${STATICLIB}" >&2
    exit 1
fi

CC="${CC:-cc}"
echo "[c-ffi-smoke] compiling with ${CC} for ${HOST_TARGET}"
"${CC}" -std=c11 -Wall -Wextra -I"${ROOT}/glib-native/include" \
    "${SRC}" "${STATICLIB}" -o "${OUT}"

echo "[c-ffi-smoke] running ${OUT}"
"${OUT}"
echo "[c-ffi-smoke] OK"
