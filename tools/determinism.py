"""Cross-platform determinism check.

Runs the release CLI on a dataset written by the `make_dataset` example:
a full generation (consistent-hash, 4 partitions, snapshot), then an
`apply` of a fixed change feed. Prints the SHA-256 of every published file
and of the snapshot after each step. CI runs it on every platform and
compares the outputs, which must be identical byte for byte.

Usage: python determinism.py BINARY DATASET WORKDIR
"""

import hashlib
import shutil
import subprocess
import sys
from pathlib import Path


def digest_tree(label, root):
    for path in sorted(p for p in root.rglob("*") if p.is_file()):
        rel = path.relative_to(root).as_posix()
        if rel.endswith(".lock") or any(part.startswith(".") for part in rel.split("/")):
            continue  # lock files: empty, and locked on Windows
        print(f"{label} {hashlib.sha256(path.read_bytes()).hexdigest()} {rel}")


def run(args):
    result = subprocess.run(args, capture_output=True, text=True)
    if result.returncode != 0:
        sys.stderr.write(result.stdout + result.stderr)
        raise SystemExit(f"{args[1]} exited with {result.returncode}")


def main():
    binary, dataset, work = sys.argv[1], sys.argv[2], Path(sys.argv[3])
    shutil.rmtree(work, ignore_errors=True)
    out, state = work / "out", work / "state"
    common = [
        "--out", str(out),
        "--state-dir", str(state),
        "--public-base-url", "https://shop.example.com/",
        "--strategy", "consistent-hash",
        "--partitions", "4",
    ]
    run([binary, "generate", "--input", dataset, *common])
    digest_tree("generate", out)
    digest_tree("generate-state", state)

    feed = work / "changes.jsonl"
    lines = []
    for i in range(0, 20_000, 997):
        loc = f"https://shop.example.com/products/item-{i}?ref=catalog&page={i % 50}"
        lines.append(f'{{"op":"upsert","url":{{"loc":"{loc}","lastmod":"2026-12-01"}}}}')
    for i in range(5, 20_000, 1_999):
        loc = f"https://shop.example.com/products/item-{i}?ref=catalog&page={i % 50}"
        lines.append(f'{{"op":"delete","loc":"{loc}"}}')
    lines.append('{"op":"upsert","url":{"loc":"https://shop.example.com/new-é?a=1&b=<2>"}}')
    feed.write_bytes(("\n".join(lines) + "\n").encode("utf-8"))
    run([binary, "apply", "--changes", str(feed), *common])
    digest_tree("apply", out)
    digest_tree("apply-state", state)


main()
