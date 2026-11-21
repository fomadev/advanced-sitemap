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
  "output": { "directory": "dist", "fileStem": "sitemap", "index": "auto", "manifest": true, "extensions": ["video"] },
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
| `partitioning.strategy` | `sequential` | `sequential` or `stable-hash` |
| `partitioning.partitions` | none | Required by `stable-hash`, refused otherwise |
| `partitioning.maxUrls` | `50000` | 1 to 50,000 |
| `partitioning.maxBytes` | `52428800` | 4,096 to 52,428,800. Lower it to keep a safety margin |
| `output.directory` | `.` | Relative to the configuration file |
| `output.fileStem` | `sitemap` | `[A-Za-z0-9_-]`, 1 to 64 characters |
| `output.index` | `auto` | `auto`, `always` or `never` |
| `output.manifest` | `true` | Write `<stem>.manifest.json` |
| `output.extensions` | `[]` | Since 1.1. Extra namespaces declared on every file: `video`. `xhtml` and `image` are always declared. A record using an undeclared extension gets `EXTENSION_NOT_DECLARED` |
| `source` / `urls` | none | A file, or inline records. The two are mutually exclusive |

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

## Manifest

`<stem>.manifest.json` lists the artifacts of the last publication: name,
kind, entries and bytes. It contains no timestamp. The next run compares
against it and reports files that are no longer produced (`STALE_ARTIFACT`).
Those files are never deleted.
