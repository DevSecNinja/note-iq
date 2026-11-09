#!/usr/bin/env python3
"""Writes a CycloneDX SBOM for each Windows release architecture.

cargo-cyclonedx also lists crates that only the tests use, the features they enable, such
as extra renderers, and proc-macros. Each SBOM keeps only the crates that
`cargo tree -e normal,no-proc-macro` reports for the release target, so it describes what
`noteiq.exe` contains, whichever platform generates it.

Requires `cargo-cyclonedx` on PATH. Set SOURCE_DATE_EPOCH for reproducible timestamps.
"""

import argparse
import json
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
MANIFEST = ROOT / "apps" / "noteiq-windows" / "Cargo.toml"
PACKAGE = "noteiq-windows"
FEATURES = "desktop"
TARGETS = {"x64": "x86_64-pc-windows-msvc", "arm64": "aarch64-pc-windows-msvc"}


def run(*args: str) -> str:
    return subprocess.run(args, cwd=ROOT, check=True, capture_output=True, text=True).stdout


def release_crates(target: str) -> set[tuple[str, str]]:
    """Every crate `cargo build` links into the release executable for `target`.

    Proc-macros run on the build machine and are not part of the executable; cargo also
    resolves their dependencies for that machine rather than for `target`, so they are
    left out.
    """
    lines = run(
        "cargo", "tree", "--locked", "--package", PACKAGE, "--features", FEATURES,
        "--target", target, "--edges", "normal,no-proc-macro", "--prefix", "none",
        "--format", "{p}",
    ).splitlines()
    crates = set()
    for line in lines:
        name, version = line.split()[:2]
        crates.add((name, version.removeprefix("v")))
    return crates


def generate(target: str) -> dict:
    run(
        "cargo", "cyclonedx", "--quiet", "--manifest-path", str(MANIFEST),
        "--format", "json", "--spec-version", "1.5", "--features", FEATURES,
        "--target", target, "--no-build-deps", "--describe", "binaries",
        "--target-in-filename",
    )
    produced = MANIFEST.parent / f"noteiq_bin_{target}.cdx.json"
    try:
        return json.loads(produced.read_text(encoding="utf-8"))
    finally:
        produced.unlink(missing_ok=True)


def prune(bom: dict, keep: set[tuple[str, str]]) -> dict:
    removed = {
        component["bom-ref"]
        for component in bom["components"]
        if (component["name"], component["version"]) not in keep
    }
    bom["components"] = [c for c in bom["components"] if c["bom-ref"] not in removed]
    bom["dependencies"] = [
        {**entry, "dependsOn": [r for r in entry.get("dependsOn", []) if r not in removed]}
        for entry in bom.get("dependencies", [])
        if entry["ref"] not in removed
    ]
    return bom


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--tag", required=True, help="Release tag, for example v0.2.0")
    parser.add_argument("--output", type=Path, required=True, help="Directory for the SBOMs")
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=True)

    for arch, target in TARGETS.items():
        keep = release_crates(target)
        root = next((name, version) for name, version in keep if name == PACKAGE)
        bom = prune(generate(target), keep)
        component = bom["metadata"]["component"]
        if (component["name"], component["version"]) != ("noteiq", root[1]):
            raise SystemExit(f"Unexpected SBOM subject for {target}: {component['name']}")
        listed = {(c["name"], c["version"]) for c in bom["components"]}
        missing = sorted(keep - listed - {root})
        if missing:
            raise SystemExit(f"The {arch} SBOM is missing release crates: {missing}")
        path = args.output / f"noteiq-{args.tag}-windows-{arch}.cdx.json"
        path.write_text(json.dumps(bom, indent=2) + "\n", encoding="utf-8")
        print(f"{path.name}: {len(bom['components'])} components")
    return 0


if __name__ == "__main__":
    sys.exit(main())
