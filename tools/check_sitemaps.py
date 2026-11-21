#!/usr/bin/env python3
"""Independent conformance checker for sitemap files.

This script shares no code with the Rust engine. It uses the Python standard
library XML parser (expat) and re-implements the rules of the sitemaps.org
protocol and of the Google extensions the engine emits, so that the expected
fixture files are not checked only by the code that produced them.

Usage:
    python tools/check_sitemaps.py FILE_OR_DIR [FILE_OR_DIR ...]

Every `*.xml` file found is checked. Exit code 0 when all files conform,
1 otherwise. Only the standard library is used.
"""

from __future__ import annotations

import re
import sys
import xml.etree.ElementTree as ET
from pathlib import Path
from urllib.parse import urlsplit

SM = "http://www.sitemaps.org/schemas/sitemap/0.9"
XHTML = "http://www.w3.org/1999/xhtml"
IMAGE = "http://www.google.com/schemas/sitemap-image/1.1"

MAX_URLS = 50_000
MAX_BYTES = 52_428_800
MAX_LOC_CHARS = 2_048  # sitemaps.org: "less than 2,048 characters"

CORE_ORDER = ["loc", "lastmod", "changefreq", "priority"]
CHANGEFREQ = {"always", "hourly", "daily", "weekly", "monthly", "yearly", "never"}

# W3C Datetime profile (https://www.w3.org/TR/NOTE-datetime): YYYY, YYYY-MM,
# YYYY-MM-DD, or a date with hh:mm, hh:mm:ss or hh:mm:ss.s and a zone.
W3C_DATETIME = re.compile(
    r"^\d{4}(-(0[1-9]|1[0-2])(-(0[1-9]|[12]\d|3[01])"
    r"(T([01]\d|2[0-3]):[0-5]\d(:[0-5]\d(\.\d+)?)?(Z|[+-]([01]\d|2[0-3]):[0-5]\d))?)?)?$"
)
# Characters XML 1.0 forbids, which a conforming file can never contain.
FORBIDDEN = re.compile("[\x00-\x08\x0b\x0c\x0e-\x1f￾￿]")
PRIORITY = re.compile(r"^(0(\.\d{1,4})?|1(\.0{1,4})?)$")


def split(tag: str) -> tuple[str, str]:
    if tag.startswith("{"):
        ns, local = tag[1:].split("}", 1)
        return ns, local
    return "", tag


def check_loc(value: str | None, where: str, errors: list[str]) -> None:
    if value is None or value != value.strip() or not value:
        errors.append(f"{where}: loc is empty or has surrounding whitespace")
        return
    if len(value) >= MAX_LOC_CHARS:
        errors.append(f"{where}: loc has {len(value)} characters (limit {MAX_LOC_CHARS - 1})")
    parts = urlsplit(value)
    if parts.scheme not in ("http", "https") or not parts.netloc:
        errors.append(f"{where}: loc is not an absolute http(s) URL: {value!r}")
    if parts.username or parts.password:
        errors.append(f"{where}: loc contains credentials")
    if any(ord(c) > 0x7E or c in ' "<>\\^`{|}' for c in value):
        errors.append(f"{where}: loc is not fully escaped (non-ASCII or unsafe characters)")


def check_url(url: ET.Element, where: str, errors: list[str]) -> None:
    core_seen: list[str] = []
    extension_started = False
    for child in url:
        ns, local = split(child.tag)
        if ns == SM:
            if extension_started:
                errors.append(f"{where}: <{local}> appears after an extension element (XSD order)")
            if local not in CORE_ORDER:
                errors.append(f"{where}: unknown sitemap element <{local}>")
                continue
            if local in core_seen:
                errors.append(f"{where}: <{local}> is repeated")
            if core_seen and CORE_ORDER.index(local) < CORE_ORDER.index(core_seen[-1]):
                errors.append(f"{where}: <{local}> is out of order (loc, lastmod, changefreq, priority)")
            core_seen.append(local)
            text = child.text or ""
            if local == "loc":
                check_loc(child.text, where, errors)
            elif local == "lastmod" and not W3C_DATETIME.match(text):
                errors.append(f"{where}: lastmod {text!r} is not a W3C Datetime")
            elif local == "changefreq" and text not in CHANGEFREQ:
                errors.append(f"{where}: changefreq {text!r} is not allowed")
            elif local == "priority" and not PRIORITY.match(text):
                errors.append(f"{where}: priority {text!r} is not in 0.0..1.0")
        else:
            extension_started = True
            check_extension(child, ns, local, where, errors)
    if not core_seen or core_seen[0] != "loc":
        errors.append(f"{where}: <loc> must be the first child")


def check_extension(el: ET.Element, ns: str, local: str, where: str, errors: list[str]) -> None:
    if ns == XHTML and local == "link":
        if el.get("rel") != "alternate":
            errors.append(f"{where}: xhtml:link rel must be 'alternate'")
        if not el.get("hreflang"):
            errors.append(f"{where}: xhtml:link has no hreflang")
        check_loc(el.get("href"), f"{where} xhtml:link", errors)
    elif ns == IMAGE and local == "image":
        locs = [c for c in el if c.tag == f"{{{IMAGE}}}loc"]
        if len(locs) != 1:
            errors.append(f"{where}: image:image must contain exactly one image:loc")
        else:
            check_loc(locs[0].text, f"{where} image:loc", errors)
    else:
        errors.append(f"{where}: unexpected extension element {{{ns}}}{local}")


def check_file(path: Path) -> list[str]:
    errors: list[str] = []
    data = path.read_bytes()
    if len(data) > MAX_BYTES:
        errors.append(f"{len(data)} bytes, above the {MAX_BYTES} byte limit")
    if not data.startswith(b'<?xml version="1.0" encoding="UTF-8"?>'):
        errors.append("missing UTF-8 XML declaration")
    try:
        text = data.decode("utf-8")
    except UnicodeDecodeError as exc:
        return errors + [f"not valid UTF-8: {exc}"]
    if FORBIDDEN.search(text):
        errors.append("contains characters forbidden by XML 1.0")
    try:
        root = ET.fromstring(data)
    except ET.ParseError as exc:
        return errors + [f"not well-formed: {exc}"]

    ns, local = split(root.tag)
    if ns != SM:
        return errors + [f"root namespace is {ns!r}, expected {SM!r}"]
    if local == "urlset":
        urls = list(root)
        if len(urls) > MAX_URLS:
            errors.append(f"{len(urls)} <url> elements, above {MAX_URLS}")
        for i, url in enumerate(urls, 1):
            if url.tag != f"{{{SM}}}url":
                errors.append(f"child {i} of <urlset> is not <url>")
                continue
            check_url(url, f"url {i}", errors)
    elif local == "sitemapindex":
        entries = list(root)
        if len(entries) > MAX_URLS:
            errors.append(f"{len(entries)} <sitemap> elements, above {MAX_URLS}")
        for i, entry in enumerate(entries, 1):
            where = f"sitemap {i}"
            if entry.tag != f"{{{SM}}}sitemap":
                errors.append(f"child {i} of <sitemapindex> is not <sitemap>")
                continue
            names = [split(c.tag)[1] for c in entry]
            if names not in (["loc"], ["loc", "lastmod"]):
                errors.append(f"{where}: children must be loc[, lastmod], got {names}")
            for c in entry:
                if split(c.tag)[1] == "loc":
                    check_loc(c.text, where, errors)
                elif not W3C_DATETIME.match(c.text or ""):
                    errors.append(f"{where}: lastmod {c.text!r} is not a W3C Datetime")
    else:
        errors.append(f"unknown root element <{local}>")
    return errors


def main(argv: list[str]) -> int:
    if not argv:
        print(__doc__, file=sys.stderr)
        return 2
    files: list[Path] = []
    for arg in argv:
        p = Path(arg)
        files.extend(sorted(p.rglob("*.xml")) if p.is_dir() else [p])
    if not files:
        print("no XML file found", file=sys.stderr)
        return 2
    failed = 0
    for f in files:
        errors = check_file(f)
        if errors:
            failed += 1
            for e in errors:
                print(f"FAIL {f}: {e}")
        else:
            print(f"ok   {f}")
    print(f"{len(files) - failed}/{len(files)} files conform")
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
