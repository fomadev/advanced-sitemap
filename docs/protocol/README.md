# JSON protocol, version 1.1

The protocol is the contract between the engine and any language (ADR-0001).
JSON Schemas (draft 2020-12) are in [`schemas/`](../../schemas).

## Versioning

- Every document has `"schemaVersion": "MAJOR.MINOR"`.
- The engine accepts major `1` with a minor up to the minor it supports
  (currently `1`). A document with a newer minor is refused with a version
  error. Its fields are not silently ignored.
- Unknown fields are refused, which catches typos such as `lastMod`.
- Adding an optional field is a minor change. Removing or renaming a field,
  or changing its meaning, is a major change.

## Configuration document

```json
{
  "schemaVersion": "1.1",
  "site": "https://example.com",
  "publicBaseUrl": "https://example.com/sitemaps/",
  "referenceTime": "2026-11-21T00:00:00Z",
  "validation": { "mode": "standard", "onInvalid": "fail", "dedupe": true, "maxDiagnostics": 1000 },
  "partitioning": { "strategy": "sequential", "maxUrls": 50000, "maxBytes": 52428800 },
  "output": { "directory": "dist", "fileStem": "sitemap", "index": "auto", "manifest": true, "extensions": ["video", "news"] },
  "source": { "format": "jsonl", "path": "urls.jsonl", "maxLineBytes": 1048576 }
}
```

| Field | Default | Meaning |
|---|---|---|
| `site` | none | Expected origin of page URLs. When set, other origins get `HOST_MISMATCH` |
| `publicBaseUrl` | none | Public URL of the output directory. Required when an index is written. A trailing `/` is added if missing |
| `referenceTime` | none | Instant used by time rules (`LASTMOD_IN_FUTURE`). The engine never reads the clock |
| `validation.mode` | `standard` | `strict` makes https and same-origin mandatory and requires `lastmod`, `changefreq` and `priority` |
| `validation.onInvalid` | `fail` | `fail`: publish nothing if any record is invalid. `skip`: exclude invalid records |
| `validation.dedupe` | `true` | Drop repeated URLs and keep the first. Costs 17 to 34 bytes of memory per distinct URL (see performance) |
| `validation.maxDiagnostics` | `1000` | Diagnostics listed in the report. Counts stay exact |
| `validation.hreflangReciprocity` | `false` | Since 1.1. Check across entries that every alternate links back (`HREFLANG_RETURN_LINK_MISSING`) and that pages agree on language codes (`HREFLANG_CONFLICT`). Costs about 110 bytes per alternate link |
| `partitioning.strategy` | `sequential` | `sequential`, `stable-hash` or `consistent-hash` (since 1.1) |
| `partitioning.partitions` | none | Required by `stable-hash` and `consistent-hash`, refused otherwise |
| `partitioning.maxUrls` | `50000` | 1 to 50,000 |
| `partitioning.maxBytes` | `52428800` | 4,096 to 52,428,800. Lower it to keep a safety margin |
| `output.directory` | `.` | Relative to the configuration file |
| `output.fileStem` | `sitemap` | `[A-Za-z0-9_-]`, 1 to 64 characters |
| `output.index` | `auto` | `auto`, `always` or `never` |
| `output.manifest` | `true` | Write `<stem>.manifest.json` |
| `output.publish` | `all` | Since 1.1. `changed`: files whose bytes equal the published file are not rewritten (same result, less disk I/O). The report marks them `"unchanged": true`. `auto`: `changed` once a publication exists, `all` before; the report's `execution` says which and why |
| `output.extensions` | `[]` | Since 1.1. Extra namespaces declared on every file: `video`, `news`. `xhtml` and `image` are always declared. A record using an undeclared extension gets `EXTENSION_NOT_DECLARED` |
| `source` / `urls` | none | A file, or inline records. The two are mutually exclusive |
| `robots` | none | Since 1.1. Publishes `robots.txt` next to the sitemaps (see below) |
| `state.directory` | none | Since 1.1. Keeps a URL-level snapshot (`<stem>.snapshot`) there and adds `changes` to the report. Relative to the configuration file; keep it out of the public output directory |
| `state.maxListed` | `1000` | URLs listed per change kind in `changes` (counts stay exact) |

### robots.txt (since 1.1)

```json
"robots": {
  "groups": [
    { "userAgents": ["Googlebot", "Bingbot"], "disallow": ["/private/"], "allow": ["/private/press-kit.html"] },
    { "userAgents": ["*"], "disallow": ["/admin/"] }
  ],
  "sitemaps": ["https://example.com/news-sitemap.xml"]
}
```

The engine writes `robots.txt` (RFC 9309) into the output directory and
publishes it with the sitemaps:

- Groups are written in order: `User-agent` lines, then `Disallow`, then
  `Allow`, each in input order. A blank line separates groups. An empty
  path (`""`) is written as `Disallow:` and allows everything.
- The generated sitemap (the index, or the single sitemap file) is added as
  a `Sitemap` record when `publicBaseUrl` is set. Otherwise
  `ROBOTS_SITEMAP_NOT_LISTED` is reported. URLs in `sitemaps` must be
  absolute; duplicates are removed.
- Paths must be empty or start with `/` or `*`. Non-ASCII characters are
  percent-encoded (UTF-8). `*` and `$` keep their RFC 9309 meaning.
- User agents follow the RFC 9309 product token (letters, `-`, `_`) or
  `*`. Other characters such as digits get a warning, spaces or `:` an
  error. A user agent repeated in two groups gets a warning (crawlers
  merge the groups).
- Errors in `robots` are configuration errors (exit code 2): nothing is
  generated. A file above 500 KiB gets `ROBOTS_TOO_LARGE`.
- `robots.txt` is only useful at the root of the host. If the output
  directory is not served at `/`, copy the file there.
- No timestamp is written, so the file is reproducible. `Crawl-delay` and
  `Host` are not supported: they are not part of RFC 9309.

## URL record

```json
{
  "loc": "https://example.com/fr/",
  "lastmod": "2026-11-21T09:30:00+01:00",
  "changefreq": "weekly",
  "priority": 0.8,
  "alternates": [
    { "hreflang": "fr", "href": "https://example.com/fr/" },
    { "hreflang": "x-default", "href": "https://example.com/" }
  ],
  "images": [ { "loc": "https://cdn.example.com/cover.png" } ]
}
```

- `url` is accepted as an alias of `loc`, for migration from
  `next-advanced-sitemap`.
- Records are decoded one at a time. A malformed record produces
  `INPUT_PARSE_ERROR` and does not hide the records after it.
- Image fields other than `loc` (`title`, `caption`, `geo_location`,
  `license`) are refused because Google no longer uses them.

### Videos (since 1.1)

```json
{
  "loc": "https://example.com/videos/grilling",
  "videos": [
    {
      "thumbnailLoc": "https://example.com/thumbs/123.jpg",
      "title": "Grilling steaks for summer",
      "description": "How to get perfectly done steaks every time.",
      "contentLoc": "https://streamserver.example.com/video123.mp4",
      "playerLoc": "https://www.example.com/videoplayer.php?video=123",
      "duration": 460,
      "expirationDate": "2030-11-05T19:20:30+08:00",
      "rating": 4.2,
      "viewCount": 12345,
      "publicationDate": "2026-02-28T19:20:30+08:00",
      "tags": ["steak", "summer"],
      "familyFriendly": true,
      "restriction": { "relationship": "allow", "countries": ["IE", "GB"] },
      "requiresSubscription": false,
      "uploader": { "name": "GrillyMcGrillerson", "info": "https://example.com/users/grilly" },
      "platform": { "relationship": "deny", "platforms": ["tv"] },
      "live": false
    }
  ]
}
```

The configuration must list `"video"` in `output.extensions`. Rules follow
Google's video sitemap documentation and its `sitemap-video/1.1` XSD:

- `thumbnailLoc`, `title` and `description` are required, and at least one
  of `contentLoc` and `playerLoc`. Neither may equal `loc`.
- `description` has at most 2,048 characters. A `title` longer than 100
  characters gets a warning: the XSD refuses it, the documentation states
  no limit.
- `duration` is 1 to 28,800 seconds, `rating` 0.0 to 5.0, at most 32 `tags`.
- Country codes are two letters (format only, written in uppercase).
  Duplicates in `countries` and `platforms` are removed.
- `uploader.name` has 1 to 255 characters. `uploader.info` on another host
  than `loc` gets a warning.
- With `referenceTime`, an `expirationDate` that is not later gets
  `VIDEO_EXPIRED` (warning).
- Booleans are written `yes` / `no`. Children are written in the XSD order.
- `category`, `gallery_loc`, `price`, `tvshow` and the `player_loc`
  attributes `allow_embed` and `autoplay` are not supported: Google removed
  them from its documentation in 2022.

### News (since 1.1)

```json
{
  "loc": "https://www.example.org/business/article55.html",
  "news": {
    "publication": { "name": "The Example Times", "language": "en" },
    "publicationDate": "2026-11-20T19:20:30+01:00",
    "title": "Companies A, B in Merger Talks"
  }
}
```

The configuration must list `"news"` in `output.extensions`. Rules follow
Google's news sitemap documentation and its `sitemap-news/0.9` XSD:

- All four values are required. `name` must match the publication name on
  Google News, without anything in parentheses (not checkable by the
  engine).
- `language` is an ISO 639 code of two or three letters, or `zh-cn` /
  `zh-tw`. It is written in lowercase (the XSD pattern). Regional codes such
  as `en-gb` are refused.
- At most 1,000 news entries per file. The `sequential` strategy starts a
  new file at the 1,001st news entry; `stable-hash` fails with
  `PARTITION_LIMIT_EXCEEDED` (raise `partitions`).
- With `referenceTime`, an article older than two days gets `NEWS_TOO_OLD`
  (warning): Google News only uses articles from the last two days.
- `access`, `genres`, `keywords` and `stock_tickers` exist in the XSD but
  are no longer in Google's documentation; they are not supported.

## Report

`generate` and `validate` produce:

```json
{
  "schemaVersion": "1.0",
  "engineVersion": "0.1.0",
  "status": "published | failed | valid | invalid",
  "stats": { "records": 5, "validRecords": 5, "invalidRecords": 0, "duplicates": 0, "urlsWritten": 5 },
  "artifacts": [ { "name": "sitemap-1.xml", "kind": "sitemap", "entries": 2, "bytes": 401 } ],
  "store": { "kind": "local-fs", "location": "...", "atomicPerArtifact": true, "atomicSet": false },
  "severityCounts": { "error": 0, "warning": 0, "recommendation": 0, "info": 0 },
  "diagnosticCounts": { "DUPLICATE_URL": 1 },
  "diagnostics": [
    {
      "code": "LASTMOD_INVALID",
      "severity": "error",
      "message": "lastmod is not a W3C Datetime: expected a four-digit year (YYYY)",
      "location": { "record": 2, "line": 2, "field": "lastmod" },
      "value": "21/11/2026",
      "suggestion": "Use YYYY-MM-DD or YYYY-MM-DDThh:mm:ss+hh:mm (time zone required with a time)."
    }
  ]
}
```

## Hash strategies

Both place a URL from `fnv1a64(loc)` alone, so a URL keeps its file while
`partitions` is unchanged, and a full partition fails the run
(`PARTITION_LIMIT_EXCEEDED`).

| Strategy | Partition of a URL | Growing `partitions` from N to N+1 |
|---|---|---|
| `stable-hash` | `fnv1a64(loc) mod N`, plus 1 | Almost every URL changes file (N/(N+1)) |
| `consistent-hash` | `jump(fnv1a64(loc), N)`, plus 1 (Lamping and Veach, 2014) | About 1/(N+1) of the URLs move, all to the new partition N+1 |

Both functions are frozen by test vectors. Switching from one strategy to
the other moves almost every URL once. Every existing file still changes
when N grows, because each one gives some URLs to the new partition.

## Manifest digests and snapshot version (since 1.1)

- Each manifest entry has `digest`, the BLAKE3-128 digest (hex) of the
  file's bytes, so the manifest describes the content of the publication
  and can be used to check a deployment. Reports carry the same `digest`
  per artifact. Manifests written before 1.1 have no digests; they are
  still read.
- The URL snapshot is format version 2 (`ASNAPv2`): its header records the
  digest of the manifest of the publication it describes. A run
  interrupted after publishing, before the snapshot was replaced, is then
  detected: the next change report warns with `SNAPSHOT_STALE`, and
  `apply` refuses before reading any file. Version 1 snapshots are still
  read (without this check).

## Change feeds (since 1.1)

The `apply` command reads JSON Lines operations, one per line, validated by
[`schemas/change-record.v1.schema.json`](../../schemas/change-record.v1.schema.json):
`{"op": "upsert", "url": <URL record>}` or `{"op": "delete", "loc": "..."}`.
Unknown fields and operations are rejected per line, like a malformed
source record. See [the CLI](../cli/README.md#apply-delta-mode).

## Change report (`changes`, since 1.1)

With `state.directory`, the report gains:

```json
"changes": {
  "previousSnapshot": true,
  "configChanged": false,
  "added": 1, "removed": 0, "modified": 2, "moved": 0, "unchanged": 999997,
  "partitionsTouched": [3, 10],
  "addedUrls": ["https://example.com/new"],
  "removedUrls": [],
  "modifiedUrls": ["https://example.com/a", "https://example.com/b"],
  "truncated": false
}
```

- A URL is *modified* when its encoded `<url>` block changes (any field,
  extensions included), *moved* when only its partition changes.
- `previousSnapshot: false`: no readable snapshot was found, so every URL
  counts as added. A damaged snapshot is reported with `SNAPSHOT_INVALID`
  and replaced.
- `configChanged`: a setting that shapes the output changed (limits,
  strategy, partitions, namespaces, file stem, index mode, public base URL,
  engine version).
- The snapshot is replaced only after a successful publication. `validate`
  (dry run) reports changes and leaves it untouched.

## Manifest

`<stem>.manifest.json` lists the artifacts of the last publication: name,
kind, entries and bytes. It contains no timestamp. The next run compares
against it and reports files that are no longer produced (`STALE_ARTIFACT`).
Those files are never deleted.
