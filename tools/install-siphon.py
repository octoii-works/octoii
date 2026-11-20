#!/usr/bin/env python3
"""Install the public native archive for the SDK pinned by this workspace."""

import hashlib
import io
import json
import os
from pathlib import Path
import re
import subprocess
import tempfile
import urllib.request
import zipfile


ROOT = Path(__file__).resolve().parent.parent
RELEASES = "https://github.com/octoii-works/octoii-siphon-lib/releases/download"
ABI = 6


def version():
    lock = (ROOT / "Cargo.lock").read_text(encoding="utf-8")
    match = re.search(r'^name = "octoii-siphon-lib"\nversion = "([^"]+)"$', lock, re.MULTILINE)
    if match is None:
        raise SystemExit("Cargo.lock does not pin octoii-siphon-lib")
    return match[1]


def host_target():
    version_info = subprocess.check_output(["rustc", "-vV"], cwd=ROOT, text=True)
    return next(line[6:] for line in version_info.splitlines() if line.startswith("host: "))


def files(target):
    if target == "x86_64-pc-windows-msvc":
        return {"siphon_native.lib": "sha256", "siphon_ffi.dll": "dll_sha256"}
    if target in {
        "aarch64-apple-darwin",
        "x86_64-apple-darwin",
        "aarch64-unknown-linux-gnu",
        "x86_64-unknown-linux-gnu",
    }:
        return {"libsiphon_native.a": "sha256"}
    raise SystemExit(f"No published native distribution for {target}")


def verify(directory, target, names):
    try:
        manifest = json.loads((directory / "manifest.json").read_text(encoding="utf-8"))
        return manifest["abi"] == ABI and manifest["target"] == target and all(
            hashlib.sha256((directory / name).read_bytes()).hexdigest() == manifest[checksum]
            for name, checksum in names.items()
        )
    except (OSError, ValueError, KeyError, TypeError):
        return False


def install(target, release):
    names = files(target)
    build = ROOT / os.environ.get("CARGO_TARGET_DIR", "target")
    cache = (build / "public-native" / release).resolve()
    cache.mkdir(parents=True, exist_ok=True)
    destination = cache / target
    if verify(destination, target, names):
        return destination
    url = f"{RELEASES}/v{release}/siphon-native-{target}.zip"
    with urllib.request.urlopen(url, timeout=90) as response:
        archive = response.read()
    with tempfile.TemporaryDirectory(dir=cache, prefix=f".{target}-") as temporary:
        staged = Path(temporary) / target
        staged.mkdir()
        with zipfile.ZipFile(io.BytesIO(archive)) as package:
            for filename in (*names, "manifest.json"):
                (staged / filename).write_bytes(package.read(filename))
        if not verify(staged, target, names):
            raise SystemExit(f"Invalid native binary distribution from {url}")
        if not destination.exists():
            try:
                staged.rename(destination)
            except OSError:
                if not verify(destination, target, names):
                    raise
        elif not verify(destination, target, names):
            for filename in (*names, "manifest.json"):
                (staged / filename).replace(destination / filename)
    return destination


if __name__ == "__main__":
    print(install(host_target(), version()))
