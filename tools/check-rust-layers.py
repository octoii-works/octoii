#!/usr/bin/env python3
"""Check Octoii's dependency layers and the public runtime boundary."""

import json
from pathlib import Path
import subprocess
import sys

ROOT = Path(__file__).resolve().parent.parent
FOUNDATION = {"siphon-script", "siphon-shell", "siphon-text", "siphon-image"}
PRIVATE = {"beak", "siphon-ffi", "siphon-runtime"}
REGISTRY = "registry+https://github.com/rust-lang/crates.io-index"


def layer(package):
    path = Path(package["manifest_path"]).resolve().relative_to(ROOT)
    if path.parts[0] in {"libs", "extensions", "apps"}:
        return path.parts[0]
    raise ValueError(f"Unclassified workspace package: {path}")


def allowed(source, target):
    source_layer, target_layer = layer(source), layer(target)
    if source_layer == "apps":
        return target_layer != "apps"
    if source_layer == "extensions":
        return target_layer == "extensions" or target["name"] == "octoii"
    if source["name"] in FOUNDATION:
        return target["name"] in FOUNDATION
    if source["name"] == "octoii":
        return target["name"] in FOUNDATION
    raise ValueError(f"Product library needs a dependency rule: {source['name']}")


def main():
    result = subprocess.run(
        ["cargo", "metadata", "--locked", "--offline", "--format-version", "1"],
        cwd=ROOT, check=True, capture_output=True, text=True,
    )
    metadata = json.loads(result.stdout)
    members = set(metadata["workspace_members"])
    packages = [package for package in metadata["packages"] if package["id"] in members]
    local = {Path(package["manifest_path"]).resolve().parent: package for package in packages}
    violations = []
    for package in metadata["packages"]:
        if package["name"] in PRIVATE or package["name"].startswith("beak-"):
            violations.append(f"Private source present in the resolved graph: {package['name']}")
    for package in packages:
        source = package["name"]
        # Includes build, development, optional and target-specific dependencies.
        for dependency in package["dependencies"]:
            name = dependency["name"]
            if name == "octoii-siphon-lib":
                if source != "octoii" or dependency.get("source") != REGISTRY:
                    violations.append(f"{source} -> {name} (only Octoii may use the published SDK)")
            if name == "octoii-ai" and dependency.get("source") != REGISTRY:
                violations.append(f"{source} -> {name} (use the published AI package)")
            if "path" in dependency:
                target = local.get(Path(dependency["path"]).resolve())
                if target is None or not allowed(package, target):
                    violations.append(f"{source} -> {name} ({dependency['kind'] or 'normal'})")
    if violations:
        sys.exit("Forbidden Rust dependencies:\n" + "\n".join(violations))
    print(f"Rust dependency boundaries passed ({len(packages)} product packages).")


if __name__ == "__main__":
    main()
