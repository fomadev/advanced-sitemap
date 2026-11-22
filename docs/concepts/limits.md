# Resource limits

Every input the engine reads and every structure it holds is bounded.
Going past a limit is refused with a diagnostic or an error, never by
running out of memory or writing an invalid file. This page lists the
limits, their defaults and how to change them.

## Protocol limits (fixed)

Set by the sitemaps.org protocol and Google's extensions; they cannot be
raised.

| Limit | Value | When exceeded |
|---|---|---|
| URLs per sitemap file | 50,000 | A new file is started (`sequential`) or the run fails (`PARTITION_LIMIT_EXCEEDED`, hash strategies) |
| Uncompressed size per sitemap file | 50 MiB (52,428,800 bytes) | Same |
| Files per sitemap index | 50,000 | `INDEX_LIMIT_EXCEEDED`, nothing published |
| URL length | 2,047 characters | `URL_TOO_LONG` |
| Images per URL | 1,000 | `IMAGE_LIMIT_EXCEEDED` |
| News entries per file | 1,000 | New file (`sequential`) or `PARTITION_LIMIT_EXCEEDED` |
| News age | 2 days (with a reference time) | `NEWS_TOO_OLD` |
| Video title, description, tags, duration, uploader | 100 chars, 2,048 chars, 32 tags, 28,800 s, 255 chars | `VIDEO_*` diagnostics |
| robots.txt size | 500 KiB | `ROBOTS_TOO_LARGE` |

## Configurable limits

| Limit | Default | Setting | Notes |
|---|---|---|---|
| URLs per file | 50,000 | `partitioning.maxUrls`, `--max-urls` | 1 to 50,000 |
| Bytes per file | 52,428,800 | `partitioning.maxBytes`, `--max-bytes` | 4,096 to 52,428,800; lower it to keep a margin |
| Partitions (hash strategies) | none | `partitioning.partitions`, `--partitions` | `explain` suggests a value |
| JSONL line length | 1 MiB | `source.maxLineBytes` | A longer line is skipped with `INPUT_LINE_TOO_LONG`; also bounds the alternates and images of one record |
| JSON array document | 256 MiB | library (`JsonArrayReader::from_reader`) | JSON arrays are held in memory; use JSONL for large inputs |
| Diagnostics listed | 1,000 | `validation.maxDiagnostics`, `--max-diagnostics` | Counts stay exact |
| Snapshot sort memory | 64 MiB | library (`SnapshotSettings::memory_budget`) | Records beyond it are sorted on disk in the state directory |
| URLs listed per change kind | 1,000 | `state.maxListed` | Counts stay exact |
| URLs changed by one `apply` | 500,000 | `state.maxFeedOperations`, `apply --max-operations` | The feed is held in memory, about 800 bytes per changed URL (measured); a larger feed is refused before writing |
| Bytes read per file (`inspect`, `diff`) | 50 MiB | `--max-bytes` | Counted after gzip decompression (decompression bombs stop there) |
| XML nesting depth (reader) | 32 | library (`ReaderLimits`) | Sitemaps need 5 |
| Text of one XML element (reader) | 64 KiB | library (`ReaderLimits`) | |

## Memory that grows with the input

Everything else is bounded by the limits above. Three things grow with
the number of URLs, by design, and are measured in
[docs/performance](../performance/README.md):

| What | Cost | How to avoid it |
|---|---|---|
| Deduplication | 17 to 34 bytes per distinct URL | `validation.dedupe = false` (duplicates are then written twice) |
| hreflang reciprocity | about 110 bytes per alternate link | Off by default |
| JSON array input | the whole document | Use JSONL |

## Not limited

- **Run time.** The engine has no time limit; it never waits on the
  network. Use the scheduler or CI timeout around it.
- **Concurrency.** A run uses one thread. Concurrent runs on the same
  directory are refused (exit code 3, `busy`).
