#!/usr/bin/env python3
"""Resolve Alpine apk package names and virtual dependencies from APKINDEX."""

from __future__ import annotations

import os
import re
import sys
import tarfile
from dataclasses import dataclass
from io import BytesIO
from urllib.request import urlopen


MIRROR = os.environ.get("ALPINE_MIRROR", "https://dl-cdn.alpinelinux.org/alpine")
VERSION = os.environ.get("ALPINE_VERSION", "v3.21")
ARCH = os.environ.get("ALPINE_ARCH", "x86_64")
REPOS = ("main", "community")


@dataclass
class Package:
    deps: set[str]
    provides: set[str]


def dep_name(token: str) -> str:
    token = token.strip()
    if not token or token.startswith("!"):
        return ""
    return re.split(r"[<>=~]", token, maxsplit=1)[0]


def flush_record(
    record: dict[str, str],
    packages: dict[str, Package],
    providers: dict[str, set[str]],
) -> None:
    name = record.get("P", "")
    if not name:
        return

    deps = {dep for dep in (dep_name(token) for token in record.get("D", "").split()) if dep}
    provides = {
        dep for dep in (dep_name(token) for token in record.get("p", "").split()) if dep
    }
    packages[name] = Package(deps=deps, provides=provides)

    providers.setdefault(name, set()).add(name)
    for provided in provides:
        providers.setdefault(provided, set()).add(name)


def fetch_index(repo: str) -> tuple[dict[str, Package], dict[str, set[str]]]:
    url = f"{MIRROR}/{VERSION}/{repo}/{ARCH}/APKINDEX.tar.gz"
    data = urlopen(url, timeout=120).read()
    packages: dict[str, Package] = {}
    providers: dict[str, set[str]] = {}

    with tarfile.open(fileobj=BytesIO(data), mode="r:gz") as tf:
        member = tf.extractfile("APKINDEX")
        if member is None:
            return packages, providers
        text = member.read().decode("utf-8", errors="replace")

    record: dict[str, str] = {}
    for line in text.splitlines():
        if not line.strip():
            flush_record(record, packages, providers)
            record = {}
            continue
        if ":" in line:
            key, val = line.split(":", 1)
            record[key] = val.strip()

    flush_record(record, packages, providers)
    return packages, providers


def merge_indexes() -> tuple[dict[str, Package], dict[str, set[str]]]:
    packages: dict[str, Package] = {}
    providers: dict[str, set[str]] = {}
    for repo in REPOS:
        repo_packages, repo_providers = fetch_index(repo)
        packages.update(repo_packages)
        for provided, names in repo_providers.items():
            providers.setdefault(provided, set()).update(names)
    return packages, providers


def choose_provider(token: str, providers: dict[str, set[str]]) -> str | None:
    choices = sorted(providers.get(token, ()))
    if not choices:
        return None
    exact = [name for name in choices if name == token]
    if exact:
        return exact[0]
    return choices[0]


def resolve(
    packages: dict[str, Package],
    providers: dict[str, set[str]],
    roots: list[str],
) -> tuple[list[str], list[str]]:
    resolved: set[str] = set()
    missing: set[str] = set()
    stack = list(reversed(roots))

    while stack:
        token = stack.pop()
        package_name = choose_provider(token, providers)
        if package_name is None:
            missing.add(token)
            continue
        if package_name in resolved:
            continue
        resolved.add(package_name)
        stack.extend(sorted(packages[package_name].deps, reverse=True))

    return sorted(resolved), sorted(missing)


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

    packages, providers = merge_indexes()
    resolved, missing = resolve(packages, providers, roots)
    if missing:
        print(f"WARNING: unresolved dependencies: {', '.join(missing)}", file=sys.stderr)
    for name in resolved:
        print(name)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
