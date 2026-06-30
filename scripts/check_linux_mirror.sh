#!/bin/bash
# Compare the Linux driver source tree against RustOS driver module names.
set -euo pipefail

LINUX_ROOT="${LINUX_MASTER_DIR:-/home/justin/Downloads/linux-master}"
RUSTOS_ROOT="${RUSTOS_ROOT:-$(cd "$(dirname "$0")/.." && pwd)}"

if [[ ! -d "$LINUX_ROOT/drivers" ]]; then
    echo "Linux drivers tree not found: $LINUX_ROOT/drivers" >&2
    exit 1
fi

tmpdir="$(mktemp -d "${TMPDIR:-/tmp}/rustos-linux-mirror.XXXXXX")"
trap 'rm -rf "$tmpdir"' EXIT

find "$LINUX_ROOT/drivers" -mindepth 1 -maxdepth 1 -type d -printf '%f\n' \
    | sed 's/-/_/g' \
    | sort -u > "$tmpdir/linux-drivers.txt"

perl -ne 'print "$1\n" if /^pub mod ([A-Za-z0-9_]+);/' \
    "$RUSTOS_ROOT/src/drivers/mod.rs" \
    | sort -u > "$tmpdir/rustos-driver-mods.txt"

cat > "$tmpdir/rustos-driver-aliases.txt" <<'ALIASES'
base
bluetooth
cpufreq
cpuidle
drivers
md
net
ALIASES

sort -u "$tmpdir/rustos-driver-mods.txt" "$tmpdir/rustos-driver-aliases.txt" \
    > "$tmpdir/rustos-effective-driver-mods.txt"

comm -23 "$tmpdir/linux-drivers.txt" "$tmpdir/rustos-effective-driver-mods.txt" \
    > "$tmpdir/missing.txt"

linux_count="$(wc -l < "$tmpdir/linux-drivers.txt")"
rustos_count="$(wc -l < "$tmpdir/rustos-effective-driver-mods.txt")"
missing_count="$(wc -l < "$tmpdir/missing.txt")"

echo "Linux driver dirs: $linux_count"
echo "RustOS effective driver modules: $rustos_count"
echo "Pending driver mirrors: $missing_count"

if [[ "$missing_count" -ne 0 ]]; then
    echo
    echo "Pending Rust-owned driver mirror modules:"
    sed 's/^/  drivers\//' "$tmpdir/missing.txt"
fi
