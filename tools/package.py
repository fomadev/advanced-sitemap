"""Packages a release candidate of the CLI.

Optionally checks the binary first (--smoke): `--version` reports the
workspace version, and `generate` reproduces the expected files of the
conformance fixtures byte for byte. Then writes
`advanced-sitemap-<version>-<commit>-<target>.(tar.gz|zip)` with the
binary, LICENSE, THIRD-PARTY-NOTICES.txt (tools/notices.py), README.md,
CHANGELOG.md and BUILDINFO.txt, and its `.sha256`. Archive members have fixed timestamps and owners, so the
archive depends only on the files it contains.

Usage: python package.py --target T --binary PATH --commit SHA --out DIR [--smoke]
"""

import argparse
import gzip
import hashlib
import io
import subprocess
import sys
import tarfile
import tempfile
import tomllib
import zipfile
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
FIXTURES = ["basic", "partitioned", "extensions", "news", "video", "robots"]
# 2026-01-01T00:00:00Z: fixed member time.
EPOCH = 1767225600


def version():
    with open(ROOT / "Cargo.toml", "rb") as f:
        return tomllib.load(f)["workspace"]["package"]["version"]


def listing(directory):
    return sorted(
        p.relative_to(directory).as_posix()
        for p in directory.rglob("*")
        if p.is_file() and not p.name.startswith(".")
    )


def smoke(binary, expected_version):
    out = subprocess.run([binary, "--version"], capture_output=True, text=True, check=True)
    if expected_version not in out.stdout:
        raise SystemExit(f"--version printed {out.stdout!r}, expected {expected_version}")
    for name in FIXTURES:
        fixture = ROOT / "fixtures" / "conformance" / name
        with tempfile.TemporaryDirectory() as tmp:
            result = subprocess.run(
                [binary, "generate", "--config", str(fixture / "config.json"), "--out", tmp],
                capture_output=True,
                text=True,
            )
            if result.returncode != 0:
                raise SystemExit(f"{name}: exit {result.returncode}\n{result.stdout}{result.stderr}")
            expected = fixture / "expected"
            got, want = listing(Path(tmp)), listing(expected)
            if got != want:
                raise SystemExit(f"{name}: files {got} != {want}")
            for rel in want:
                if (Path(tmp) / rel).read_bytes() != (expected / rel).read_bytes():
                    raise SystemExit(f"{name}: {rel} differs from the fixture")
        print(f"smoke: {name} ok")


def rustc_version():
    try:
        return subprocess.run(["rustc", "-V"], capture_output=True, text=True).stdout.strip()
    except OSError:
        return "unknown"


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--target", required=True)
    parser.add_argument("--binary", required=True)
    parser.add_argument("--commit", required=True)
    parser.add_argument("--out", required=True)
    parser.add_argument("--smoke", action="store_true")
    args = parser.parse_args()

    v = version()
    binary = Path(args.binary)
    if args.smoke:
        smoke(str(binary), v)

    stem = f"advanced-sitemap-{v}-{args.commit}-{args.target}"
    windows = "windows" in args.target
    exe = "advanced-sitemap.exe" if windows else "advanced-sitemap"
    buildinfo = (
        f"advanced-sitemap {v}\ncommit {args.commit}\ntarget {args.target}\n"
        f"compiler {rustc_version()}\n"
        "Release candidate: not a published release.\n"
    ).encode()
    with tempfile.TemporaryDirectory() as tmp:
        notices_path = Path(tmp) / "THIRD-PARTY-NOTICES.txt"
        subprocess.run(
            [sys.executable, str(ROOT / "tools" / "notices.py"),
             "--target", args.target, "--out", str(notices_path)],
            check=True,
        )
        notices = notices_path.read_bytes()
    members = [
        (exe, binary.read_bytes(), 0o755),
        ("LICENSE", (ROOT / "LICENSE").read_bytes(), 0o644),
        ("THIRD-PARTY-NOTICES.txt", notices, 0o644),
        ("README.md", (ROOT / "README.md").read_bytes(), 0o644),
        ("CHANGELOG.md", (ROOT / "CHANGELOG.md").read_bytes(), 0o644),
        ("BUILDINFO.txt", buildinfo, 0o644),
    ]

    out = Path(args.out)
    out.mkdir(parents=True, exist_ok=True)
    if windows:
        archive = out / f"{stem}.zip"
        with zipfile.ZipFile(archive, "w", zipfile.ZIP_DEFLATED) as zf:
            for name, data, mode in members:
                info = zipfile.ZipInfo(f"{stem}/{name}", date_time=(2026, 1, 1, 0, 0, 0))
                info.external_attr = (0o100000 | mode) << 16
                info.compress_type = zipfile.ZIP_DEFLATED
                zf.writestr(info, data)
    else:
        archive = out / f"{stem}.tar.gz"
        raw = io.BytesIO()
        with tarfile.open(fileobj=raw, mode="w", format=tarfile.PAX_FORMAT) as tf:
            for name, data, mode in members:
                info = tarfile.TarInfo(f"{stem}/{name}")
                info.size, info.mode, info.mtime = len(data), mode, EPOCH
                info.uid = info.gid = 0
                info.uname = info.gname = ""
                tf.addfile(info, io.BytesIO(data))
        with open(archive, "wb") as f:
            with gzip.GzipFile(filename="", mode="wb", fileobj=f, mtime=EPOCH) as gz:
                gz.write(raw.getvalue())
    digest = hashlib.sha256(archive.read_bytes()).hexdigest()
    (out / f"{archive.name}.sha256").write_text(f"{digest}  {archive.name}\n")
    print(f"{digest}  {archive.name}")


if __name__ == "__main__":
    sys.exit(main())
