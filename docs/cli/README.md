# CLI: `advanced-sitemap`

```
advanced-sitemap [--format human|json] <generate|validate|explain> [options]
```

| Command | Effect |
|---|---|
| `generate` | Validates the input and publishes sitemap files into the output directory |
| `validate` | Runs the same pipeline with a store that keeps nothing; writes no file |
| `explain` | Samples the input and describes what `generate` would do; writes no file |
| `inspect` | Reads existing sitemap or sitemap index files (plain or gzip) and checks them; writes no file |
| `diff` | Compares two sets of published sitemaps (files or directories); writes no file |

## Input

Either `--config FILE` (a [protocol v1](../protocol/README.md) document), or
`--input FILE`, or both. `--input` replaces the document's input.

| Extension | Reading |
|---|---|
| `.jsonl`, `.ndjson` | Streamed one line at a time; each line is at most 1 MiB by default |
| `.json` | A JSON array, loaded in memory (at most 256 MiB) |

Relative paths in a configuration file are resolved against the directory
of that file. Relative paths on the command line are resolved against the
current directory.

## Options

Command-line options override the configuration document.

| Option | Document field |
|---|---|
| `--out DIR` (`generate` only) | `output.directory` |
| `--site URL` | `site` |
| `--public-base-url URL` | `publicBaseUrl` |
| `--reference-time W3C-DATETIME` | `referenceTime` |
| `--strict` | `validation.mode = "strict"` |
| `--on-invalid fail\|skip` | `validation.onInvalid` |
| `--no-dedupe` | `validation.dedupe = false` |
| `--hreflang-reciprocity` | `validation.hreflangReciprocity = true` |
| `--state-dir DIR` | `state.directory` (snapshot and change report) |
| `--max-diagnostics N` | `validation.maxDiagnostics` |
| `--strategy sequential\|stable-hash\|consistent-hash` | `partitioning.strategy` |
| `--partitions N` | `partitioning.partitions` |
| `--max-urls N` | `partitioning.maxUrls` |
| `--max-bytes N` | `partitioning.maxBytes` |
| `--index auto\|always\|never` | `output.index` |
| `--file-stem NAME` | `output.fileStem` |
| `--no-manifest` (`generate` only) | `output.manifest = false` |
| `--skip-unchanged` (`generate` only) | `output.publish = "changed"` |
| `--deny-warnings` (`generate`, `validate`, `inspect`) | exit 1 if any warning or error is reported |
| `--sample N` (`explain` only) | number of records sampled, default 1000 |

## `inspect`

```sh
advanced-sitemap inspect public/sitemap.xml public/sitemap-1.xml.gz
advanced-sitemap --format json inspect --site https://example.com --strict sitemap.xml
curl -s https://example.com/sitemap.xml | advanced-sitemap inspect -
```

`inspect` reads local files, or standard input with `-`. It does not fetch
URLs: downloading needs an SSRF guard, which is planned. Gzip input is
detected from its first bytes, not from the file name.

Each file is checked for well-formedness, the root element and namespace,
the XSD element order, the 50,000-entry and 50 MiB limits, and every entry
is validated with the same rules as `generate`. Elements of extensions the
engine does not model yet (video, news) are counted and skipped, not
validated. Child sitemaps of an index are listed but not followed.

| Option | Effect |
|---|---|
| `--site URL` | Report `HOST_MISMATCH` for URLs of another origin |
| `--reference-time W3C-DATETIME` | Enable `LASTMOD_IN_FUTURE` |
| `--strict` | Strict profile for `<url>` entries |
| `--no-dedupe` | Do not report duplicate URLs |
| `--max-bytes N` | Uncompressed bytes read per file before stopping (default 52428800) |
| `--deny-warnings` | Exit 1 if any warning is reported |

Hostile input is bounded: a `DOCTYPE` stops reading (no entity is ever
expanded or fetched), the uncompressed size is capped (which also stops gzip
bombs), and nesting depth and text length are limited. See
[security](../security/README.md).

The JSON output follows `schemas/inspect-report.v1.schema.json`. A file is
`valid` when it was read to the end without any `error` diagnostic.

## `diff`

```sh
advanced-sitemap diff old-dist/ dist/
advanced-sitemap --format json diff --fail-on-change old.xml.gz new.xml
```

Each argument is a file, or a directory whose `.xml` and `.xml.gz` files
are read (not recursively). Both sides are read with the same bounded reader
as `inspect`; every valid `<url>` is re-encoded in the engine's canonical
form, so whitespace, quoting and element order do not count as changes, and
any change of a value (including extensions) does. Which file a URL is in
is not compared. Sitemap index files are read but contribute no URL.

| Option | Effect |
|---|---|
| `--max-listed N` | URLs listed per kind (default 1000; counts stay exact) |
| `--max-bytes N` | Uncompressed bytes read per file |
| `--fail-on-change` | Exit 1 when the sides differ |

Exit code 1 also when a file of either side has errors (run `inspect` on it).
Memory is bounded: both sides are sorted with the snapshot's external sort
in the system temporary directory.

## Migration cost in `explain`

With `--state-dir` (or `state.directory`) and a previous publication,
`explain` reads the previous snapshot and recomputes where each URL would
go with the configured strategy. The JSON plan gains:

```json
"migration": {
  "previousUrls": 1000000,
  "previousPartitions": 24,
  "configChanged": true,
  "moved": {"kind": "measured", "value": 40112},
  "fullestPartitionUrls": 40333
}
```

- `moved` is exact for `stable-hash` and `consistent-hash`, and `unknown`
  for `sequential`, whose placement depends on the input order. It counts
  previously published URLs only; new URLs are not known before the run.
- Notes: `PARTITION_MIGRATION_COST` when URLs would change file, and
  `PARTITION_HEADROOM_LOW` when a partition would exceed 90 % of
  `maxUrls` with the previous URLs alone. Byte and news headroom are not
  measured: the snapshot does not store sizes.

## Exit codes (stable contract)

| Code | Meaning |
|---|---|
| 0 | Success. With `onInvalid = skip`, success can include reported errors for excluded records |
| 1 | Generation failed or the input is invalid (or warnings with `--deny-warnings`) |
| 2 | Usage or configuration error (bad flag, invalid or unsupported document) |
| 3 | I/O error (missing input, unwritable output) |
| 4 | Internal error |

## Output

- `--format human` (default): a summary and the first 20 diagnostics, each
  with its value and suggested fix. Errors that stop the run go to stderr.
- `--format json`: the [report document](../protocol/README.md#report) for
  `generate` and `validate`, the plan for `explain`. Errors that stop the run
  are printed to stdout as
  `{"schemaVersion":"1.0","status":"error","error":{"kind":"config|io|internal","message":"..."}}`.

## Output files

With the default stem `sitemap`:

| Situation | Files |
|---|---|
| One file needed, `index: auto` | `sitemap.xml` |
| Several files, or `index: always` | `sitemap-1.xml` ... `sitemap-N.xml` and the index `sitemap.xml` |
| `index: never` | `sitemap.xml` (one file) or `sitemap-1.xml` ... (several) |
| Always, unless `--no-manifest` | `sitemap.manifest.json` |

While a run is in progress, files are written to
`.advanced-sitemap-staging-*` inside the output directory. That directory
is removed when the run ends.

## Not yet implemented

`inspect`, `diff`, `doctor` and `benchmark` are planned. They are not
present in the binary.
