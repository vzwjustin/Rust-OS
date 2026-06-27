#!/usr/bin/env bash
# Fast compile check for glib-native on the host toolchain.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
HOST_TARGET="${HOST_TARGET:-$(rustc -vV | sed -n 's/^host: //p')}"

if [[ -z "${HOST_TARGET}" ]]; then
    echo "error: could not determine host target from rustc -vV" >&2
    exit 1
fi

cd "${ROOT}"
echo "[glib-native] checking on host target: ${HOST_TARGET}"
exec cargo check -p glib-native --lib --target "${HOST_TARGET}" "$@"
