#!/usr/bin/env python3
"""Check workspace dependency boundaries using Cargo's resolved manifest metadata."""

import json
from pathlib import Path
import subprocess
import sys

ROOT = Path(__file__).resolve().parent.parent
FOUNDATION = {"octoii-ai", "siphon-script", "siphon-shell", "siphon-text", "siphon-image"}


def layer(path):
    relative = path.relative_to(ROOT)
    if relative.parts[:2] == ("vendor", "beak"):
        return "kernel"
    if relative.parts[0] in {"core", "extensions", "apps"}:
        return relative.parts[0]
    raise ValueError(f"Unclassified workspace package: {relative}")


def allowed(source, target):
    name, source_layer = source
    dependency, target_layer = target
    if source_layer == "apps":
        return target_layer != "apps"
    if source_layer == "kernel":
        return target_layer == "kernel"
    if source_layer == "extensions":
        return target_layer == "extensions" or dependency in {"siphon", "octoii-ai"}
    if name in FOUNDATION:
        return dependency in FOUNDATION
    if name == "siphon-runtime":
        return target_layer == "kernel"
    if name == "siphon":
        return target_layer in {"core", "kernel"}
    raise ValueError(f"Core package needs a dependency rule: {name}")


def main():
    result = subprocess.run(
        ["cargo", "metadata", "--format-version", "1", "--no-deps", "--locked", "--offline"],
        cwd=ROOT, check=True, capture_output=True, text=True,
    )
    metadata = json.loads(result.stdout)
    members = set(metadata["workspace_members"])
    packages = [p for p in metadata["packages"] if p["id"] in members]
    identities = {}
    for package in packages:
        path = Path(package["manifest_path"]).resolve().parent
        identity = (package["name"], layer(path))
        if identity[1] == "core" and identity[0] not in FOUNDATION | {"siphon", "siphon-runtime"}:
            raise ValueError(f"Core package needs a dependency rule: {identity[0]}")
        identities[path] = identity
    violations = []
    for package in packages:
        source = identities[Path(package["manifest_path"]).resolve().parent]
        # Cargo includes normal, dev, build, optional and target-specific declarations here.
        for dependency in package["dependencies"]:
            if "path" not in dependency:
                continue
            path = Path(dependency["path"]).resolve()
            target = identities.get(path)
            if target is None or not allowed(source, target):
                target_name = target[0] if target else str(path)
                kind = dependency["kind"] or "normal"
                condition = dependency["target"] or "all targets"
                violations.append(f"{source[0]} -> {target_name} ({kind}, {condition})")
    if violations:
        sys.exit("Forbidden Rust dependencies:\n" + "\n".join(violations))
    print(f"Rust dependency boundaries passed ({len(packages)} packages).")


if __name__ == "__main__":
    main()
