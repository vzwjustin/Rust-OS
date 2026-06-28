#!/bin/sh
# Extended GNOME stack boot smoke test (kernel + userspace probes).

set -euo pipefail

export RUSTOS_BOOT_LOG_PATTERNS="${RUSTOS_BOOT_LOG_PATTERNS:-GNOME runtime overlay installed|D-Bus GNOME session names registered|D-Bus message bus ready|Wayland compositor ready|display: ready 800x600x32}"
export RUSTOS_BOOT_TIMEOUT_SEC="${RUSTOS_BOOT_TIMEOUT_SEC:-240}"

exec "$(dirname "$0")/boot_smoke.sh" "$@"
