"""Writes the license notices of the third-party code in the CLI binary.

The licenses of the dependencies (MIT, Apache-2.0, Unicode-3.0...) require
their notices to accompany the binaries that contain them. This script
walks the normal (run-time) dependency graph of `sitemap-cli` for one
target, as resolved by `cargo metadata --locked`, and copies the license
files each crate ships (LICENSE*, LICENCE*, COPYING*, NOTICE*, UNLICENSE).

Not included, because their code is not in the binary: build-time
dependencies, procedural macros and what only they depend on, and the
workspace's own crates (Fordima Public License, see LICENSE).

Fails if a crate ships no license file, so that a notice is never missing
silently. Identical texts are written once and referred to afterwards.

Usage: python notices.py --target TRIPLE --out FILE
"""

import argparse
import hashlib
import json
import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
LICENSE_FILE = re.compile(r"^(licen[cs]e|copying|notice|unlicense)([-._].*)?$", re.IGNORECASE)


def metadata(target):
    out = subprocess.run(
        ["cargo", "metadata", "--format-version", "1", "--locked", "--filter-platform", target],
        cwd=ROOT,
        capture_output=True,
        text=True,
        check=True,
    )
    return json.loads(out.stdout)


def runtime_packages(meta):
    """External packages linked into the CLI binary, sorted by name."""
    packages = {p["id"]: p for p in meta["packages"]}
    nodes = {n["id"]: n for n in meta["resolve"]["nodes"]}
    workspace = set(meta["workspace_members"])
    root = next(i for i in workspace if packages[i]["name"] == "sitemap-cli")

    def proc_macro(pid):
        return any("proc-macro" in t["kind"] for t in packages[pid]["targets"])

    seen, stack = set(), [root]
    while stack:
        for dep in nodes[stack.pop()]["deps"]:
            pid = dep["pkg"]
            normal = any(k["kind"] is None for k in dep["dep_kinds"])
            if normal and pid not in seen and not proc_macro(pid):
                seen.add(pid)
                stack.append(pid)
    external = [packages[i] for i in seen if i not in workspace]
    return sorted(external, key=lambda p: (p["name"], p["version"]))


def license_files(package):
    directory = Path(package["manifest_path"]).parent
    files = sorted(
        p for p in directory.iterdir() if p.is_file() and LICENSE_FILE.match(p.name)
    )
    if package.get("license_file"):
        declared = directory / package["license_file"]
        if declared.is_file() and declared not in files:
            files.append(declared)
    return files


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--target", required=True)
    parser.add_argument("--out", required=True)
    args = parser.parse_args()

    packages = runtime_packages(metadata(args.target))
    missing = [f"{p['name']} {p['version']}" for p in packages if not license_files(p)]
    if missing:
        raise SystemExit("no license file shipped by: " + ", ".join(missing))

    lines = [
        "THIRD-PARTY NOTICES",
        "",
        f"The advanced-sitemap binary for {args.target} contains the following",
        f"{len(packages)} third-party crates. Each is distributed under the license",
        "shown, whose text and copyright notices follow. Where a crate offers a",
        "choice of licenses, all the license files it ships are reproduced.",
        "Advanced Sitemap itself is under the Fordima Public License (LICENSE).",
        "",
    ]
    for p in packages:
        lines.append(f"  {p['name']} {p['version']}  ({p['license'] or 'see license file'})")
    lines.append("")

    written = {}
    for p in packages:
        header = f"{p['name']} {p['version']}"
        lines += ["=" * 78, header, f"License: {p['license'] or 'see below'}"]
        if p.get("repository"):
            lines.append(f"Source: {p['repository']}")
        lines.append("=" * 78)
        for path in license_files(p):
            text = path.read_text(encoding="utf-8", errors="replace").strip()
            key = hashlib.sha256(text.encode()).hexdigest()
            lines.append("")
            lines.append(f"--- {path.name}")
            if key in written:
                lines.append(f"(same text as {written[key]})")
            else:
                written[key] = f"{path.name} of {header}"
                lines.append(text)
        lines.append("")

    out = Path(args.out)
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_bytes(("\n".join(lines) + "\n").encode("utf-8"))
    print(f"{len(packages)} crates, {len(written)} distinct license texts -> {out}")


if __name__ == "__main__":
    sys.exit(main())
