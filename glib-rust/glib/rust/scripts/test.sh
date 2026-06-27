#!/usr/bin/env bash
# Run glib-native unit tests on the host toolchain (std), avoiding the RustOS
# cross-target default from the repo-root `.cargo/config.toml`.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
HOST_TARGET="${HOST_TARGET:-$(rustc -vV | sed -n 's/^host: //p')}"

if [[ -z "${HOST_TARGET}" ]]; then
    echo "error: could not determine host target from rustc -vV" >&2
    exit 1
fi

cd "${ROOT}"
echo "[glib-native] testing on host target: ${HOST_TARGET}"
exec cargo test -p glib-native --lib --target "${HOST_TARGET}" -- --test-threads=1 "$@"
