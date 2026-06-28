#!/usr/bin/env python3
"""Resolve Alpine apk package names (with dependencies) from APKINDEX."""

from __future__ import annotations

import sys
import tarfile
from io import BytesIO
from urllib.request import urlopen

MIRROR = "https://dl-cdn.alpinelinux.org/alpine"
VERSION = "v3.21"
ARCH = "x86_64"
REPOS = ("main", "community")


def fetch_index(repo: str) -> dict[str, set[str]]:
    url = f"{MIRROR}/{VERSION}/{repo}/{ARCH}/APKINDEX.tar.gz"
    data = urlopen(url, timeout=120).read()
    packages: dict[str, set[str]] = {}

    with tarfile.open(fileobj=BytesIO(data), mode="r:gz") as tf:
        text = tf.extractfile("APKINDEX").read().decode("utf-8", errors="replace")

    record: dict[str, str] = {}
    for line in text.splitlines():
        if not line.strip():
            if "P" in record:
                name = record["P"]
                deps: set[str] = set()
                for token in record.get("D", "").split():
                    if token.startswith("so:") or token.startswith("cmd:") or token.startswith("pc:"):
                        continue
                    dep = token.split("=")[0].split("<")[0].split(">")[0]
                    if dep:
                        deps.add(dep)
                packages[name] = deps
            record = {}
            continue
        if ":" in line:
            key, val = line.split(":", 1)
            record[key] = val.strip()

    return packages


def merge_indexes() -> dict[str, set[str]]:
    merged: dict[str, set[str]] = {}
    for repo in REPOS:
        idx = fetch_index(repo)
        for name, deps in idx.items():
            merged.setdefault(name, set()).update(deps)
    return merged


def resolve(packages: dict[str, set[str]], roots: list[str]) -> list[str]:
    seen: set[str] = set()
    order: list[str] = []

    def visit(name: str) -> None:
        if name in seen:
            return
        if name not in packages:
            return
        seen.add(name)
        for dep in sorted(packages[name]):
            visit(dep)
        order.append(name)

    for root in roots:
        visit(root)
    return order


def main() -> int:
    roots = sys.argv[1:] or [
        "gnome-shell",
        "gnome-session",
        "gnome-settings-daemon",
        "gsettings-desktop-schemas",
        "adwaita-icon-theme",
        "cantarell-fonts",
        "mutter",
    ]
    packages = merge_indexes()
    missing = [r for r in roots if r not in packages]
    if missing:
        print(f"WARNING: packages not in index: {missing}", file=sys.stderr)
    resolved = resolve(packages, roots)
    for name in resolved:
        print(name)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
