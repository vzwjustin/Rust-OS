#!/bin/bash
# Headless boot smoke test for RustOS (serial log check)

set -euo pipefail

BOOTIMAGE="${BOOTIMAGE_PATH:-target/x86_64-rustos/debug/bootimage-rustos.bin}"
PATTERNS="${RUSTOS_BOOT_LOG_PATTERNS:-${RUSTOS_BOOT_LOG_PATTERN:-GNOME runtime overlay installed|display: ready 800x600x32}}"
TIMEOUT="${RUSTOS_BOOT_TIMEOUT_SEC:-120}"

if ! command -v qemu-system-x86_64 >/dev/null 2>&1; then
    echo "Error: qemu-system-x86_64 not found. Install QEMU (brew install qemu)."
    exit 1
fi

if [ ! -f "$BOOTIMAGE" ]; then
    echo "Error: bootimage not found at $BOOTIMAGE"
    echo "Run: make bootimage"
    exit 1
fi

echo "Running boot smoke test..."
echo "Bootimage: $BOOTIMAGE"
echo "Patterns: $PATTERNS"
echo "Timeout: ${TIMEOUT}s"

BOOTIMAGE="$BOOTIMAGE" PATTERNS="$PATTERNS" TIMEOUT="$TIMEOUT" python3 - <<'PY'
import os
import atexit
import shutil
import subprocess
import sys
import tempfile

bootimage = os.environ["BOOTIMAGE"]
patterns = [p for p in os.environ["PATTERNS"].split("|") if p]
timeout = float(os.environ["TIMEOUT"])
tmpdir = tempfile.TemporaryDirectory(prefix="rustos-boot-", dir="/tmp")
atexit.register(tmpdir.cleanup)
qemu_bootimage = os.path.join(tmpdir.name, "bootimage.bin")
shutil.copyfile(bootimage, qemu_bootimage)

cmd = [
    "qemu-system-x86_64",
    "-drive", f"format=raw,file={qemu_bootimage}",
    "-m", "512M",
    "-serial", "stdio",
    "-display", "none",
    "-device", "isa-debug-exit,iobase=0xf4,iosize=0x04",
    "-machine", "pc,accel=tcg",
    "-cpu", os.environ.get("RUSTOS_QEMU_CPU", "qemu64,+apic,+rdrand"),
    "-no-reboot",
    "-no-shutdown",
]

env = os.environ.copy()
env["TMPDIR"] = "/tmp"
proc = subprocess.Popen(cmd, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, env=env)
try:
    out, _ = proc.communicate(timeout=timeout)
except subprocess.TimeoutExpired:
    proc.terminate()
    try:
        out, _ = proc.communicate(timeout=2)
    except subprocess.TimeoutExpired:
        proc.kill()
        out, _ = proc.communicate()

out = out.decode("utf-8", errors="replace")
print(out)

missing = [pattern for pattern in patterns if pattern not in out]
if missing:
    sys.stderr.write("Boot smoke test failed: expected log line(s) not found:\n")
    for pattern in missing:
        sys.stderr.write(f"  - {pattern}\n")
    sys.exit(1)

print("Boot smoke test passed.")
PY
