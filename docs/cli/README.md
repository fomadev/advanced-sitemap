# CLI: `advanced-sitemap`

```
advanced-sitemap [--format human|json] <command> [options]
```

| Command | Effect |
|---|---|
| `generate` | Validates the input and publishes sitemap files into the output directory |
| `validate` | Runs the same pipeline with a store that keeps nothing; writes no file |
| `explain` | Samples the input and describes what `generate` would do; writes no file |
| `inspect` | Reads existing sitemap or sitemap index files (plain or gzip) and checks them; writes no file |
| `diff` | Compares two sets of published sitemaps (files or directories); writes no file |
| `apply` | Applies a change feed to the published sitemaps, rewriting only the partitions it touches (delta mode) |
| `doctor` | Checks the output and state directories against the configuration; writes no file |

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

## Configuration rules

1. **Sources, lowest to highest priority:** built-in defaults, then the
   configuration document (`--config`), then command-line options. There is
   no environment variable and no implicit configuration file.
2. **One validation for every source.** Command-line options are written
   into the document first; the result is then checked as a whole. An
   option is therefore held to the same rules as the field it replaces, and
   a conflict between an option and a field of the document is reported
   like a conflict inside the document. For example, `--strategy
   sequential` with a document that sets `partitioning.partitions` is an
   error: remove the field, there is no option that clears it.
3. **Strict documents.** Unknown fields, at any level, are errors, so a
   misspelled field is never ignored. `schemaVersion` is required; a version
   newer than the engine supports is refused rather than half understood.
4. **Nothing implicit.** The engine never reads the clock (time-dependent
   rules need `referenceTime`), never infers the site from the input, and
   never deletes a file it no longer produces (it reports
   `STALE_ARTIFACT`).
5. **Errors stop before any write.** A configuration error exits with code
   2 before the input is read or a lock is taken.
6. **Mutually exclusive inputs.** `urls` and `source` cannot both be set in
   a document; `--input` replaces either.

## Options

Command-line options override the configuration document (see the rules
above).

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
| `--execution-mode full\|skip-unchanged\|auto` (`generate`, `explain`) | `output.publish = "all"`, `"changed"` or `"auto"` |
| `--expect-state TOKEN` (`generate`, `apply`) | Refuse to run if the published state is not the one `explain` planned for |
| `--out DIR` (`explain`) | Output directory whose published state is planned for; use the same as for `generate` or `apply` |
| `--out DIR` (`doctor`) | Output directory to check |
| `--deny-warnings` (`generate`, `validate`, `apply`, `inspect`, `doctor`) | exit 1 if any warning or error is reported |
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

## Execution plan in `explain`

`explain` says which execution mode a run would use, every mode it
considered and why, with estimated costs, and a state token:

```json
"execution": {
  "requested": "auto",
  "chosen": "skip-unchanged",
  "reason": "auto: a previous publication exists; same result as `full`, without rewriting unchanged files",
  "candidates": [
    {"mode": "full", "eligible": true, "millisecondsMin": {"kind": "estimated", "value": 19706}, "...": "..."},
    {"mode": "skip-unchanged", "eligible": true, "...": "..."},
    {"mode": "delta", "eligible": false, "reasons": ["needs a change feed: run `explain --changes FEED` to plan an `apply`"]}
  ],
  "stateToken": "3f0c9a...",
  "costModel": "reference laptop, 2026-11 (docs/performance)"
}
```

- Each candidate lists its preconditions (or why it is not eligible), and
  estimated time (`millisecondsMin` and `millisecondsMax`), bytes read and
  written, memory and partitions affected. Every number says whether it
  is measured or estimated. A mode is never chosen unless its
  preconditions hold; costs only order the eligible modes.
- `explain --changes FEED` plans an `apply`: it validates the feed, finds
  the partitions it touches, checks every precondition of the delta mode
  without writing, and compares the cost with regenerating from the
  source (`generate --execution-mode auto`). The source must already
  contain the feed's changes for that to be equivalent; the planner
  cannot check it, and it never turns a feed into a full source. When
  `apply` cannot run, it says why and recommends regenerating (or
  `none` without a source).
- **State token.** Pass `stateToken` to the run with `--expect-state`: it
  refuses to start (exit code 2, `"kind": "state-changed"`) if the
  settings, the published manifest or the snapshot changed since
  `explain`. The check is made with the run's lock held.
- Estimates come from a cost model calibrated on the reference machine of
  [docs/performance](../performance/README.md#execution-planner-estimates);
  on other hardware only the comparison between modes is meaningful.

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

## `apply` (delta mode)

```sh
advanced-sitemap generate --config sitemap.json          # once, from a source sorted by URL
advanced-sitemap apply --config sitemap.json --changes changes.jsonl
advanced-sitemap apply --config sitemap.json --changes - --dry-run < changes.jsonl
```

The feed is JSON Lines
([`schemas/change-record.v1.schema.json`](../../schemas/change-record.v1.schema.json)):

```json
{"op": "upsert", "url": {"loc": "https://example.com/a", "lastmod": "2026-11-22"}}
{"op": "delete", "loc": "https://example.com/b"}
```

- Operations apply in order; the last one on a URL wins (earlier ones are
  counted in `duplicates`). An upsert replaces every field of the URL.
- Only the partitions holding a changed URL are read back from the output
  directory, checked against the snapshot, changed and rewritten, with
  their entries in URL order. The other files are not touched.
- The result is byte for byte the output of a full `generate` from the
  same URLs sorted by `loc`. Start from such a publication: a source in
  another order gives files whose untouched partitions keep that order.
- Requirements, checked before anything is written: a `stable-hash` or
  `consistent-hash` strategy, `--state-dir` with the snapshot of the
  previous publication, the same settings as that publication, the
  manifest, no `--hreflang-reciprocity`, and files named `<stem>-<n>.xml`.
  A touched file changed outside the engine, or holding elements the
  reader does not keep, is refused too. A refusal exits with code 2 and
  `"kind": "delta-refused"`; run a full `generate` instead.
- `--dry-run` checks and reports without publishing. Invalid operations
  follow `validation.onInvalid`, as source records do.
- Memory: the feed is held in memory (about 800 bytes per changed URL,
  measured), plus one partition at a time. A feed changing more than
  `state.maxFeedOperations` distinct URLs (default 500,000, about 400 MB)
  is refused before anything is written; raise it with
  `--max-operations N`, split the feed, or regenerate.

## Concurrent runs

`generate`, `validate` and `apply` take an exclusive lock on the output
directory (`.advanced-sitemap.lock`) and, with `--state-dir`, on
`<state>/<stem>.lock`, before reading or writing anything. A second run on
the same directory fails at once with exit code 3 and `"kind": "busy"`;
retry when the first one has finished. The lock is released when the
process exits, even after a crash.

- The lock file is empty and stays in the output directory. Exclude it
  from deployments. On Windows it cannot be read while a run holds it, so
  copy the output directory after the run.
- The next run removes staging directories (`.advanced-sitemap-staging-*`)
  left by a run that was killed.
- `inspect` and `diff` do not take the lock: reading a directory during a
  publication can see a mix of old and new files, each of them complete.

## `doctor`

```
advanced-sitemap doctor --config advanced-sitemap.json [--out DIR] [--state-dir DIR]
```

Reads the output directory and the state directory, and writes nothing (it
does not even create a missing directory). The checks, in this order:

| Check | Looks at | Status |
|---|---|---|
| `output-directory` | The directory exists and is writable, or will be created | `error` if it is a file or read-only |
| `lock` | Whether a run holds `.advanced-sitemap.lock` or `<state>/<stem>.lock` | `warning`: a run started now would stop with `busy` |
| `staging` | `.advanced-sitemap-staging-*` left by an interrupted run | `warning`: the next run removes them |
| `manifest` | `<stem>.manifest.json` can be read | `error` if it is damaged; `skipped` without a manifest |
| `files` | Every file of the manifest exists with its size and BLAKE3 digest | `error` if one was changed or removed outside the engine; `warning` for a manifest without digests (size only) |
| `snapshot` | The snapshot matches the settings and the published manifest | `warning` if absent, unreadable, written with other settings, or stale (`SNAPSHOT_STALE`); `apply` is refused until the next `generate` |

The report also gives the state token a run would start from (the
`stateToken` of `explain`). With `--format json`:
`{"schemaVersion":"1.0","command":"doctor","status":"ok|warning|error","checks":[{"name","status","message","suggestion"}],"stateToken":"..."}`.

Exit code 0 when no check is an `error`, 1 otherwise (or on a warning with
`--deny-warnings`). Every published file is read once to check its digest:
on a large site the cost is about that of reading the sitemaps from disk.

## Exit codes (stable contract)

| Code | Meaning |
|---|---|
| 0 | Success. With `onInvalid = skip`, success can include reported errors for excluded records |
| 1 | Generation failed or the input is invalid (or warnings with `--deny-warnings`) |
| 2 | Usage or configuration error (bad flag, invalid or unsupported document), a refused `apply`, or a state that changed since `explain` (`--expect-state`) |
| 3 | I/O error (missing input, unwritable output), or the output or state directory is locked by another run (`"kind": "busy"`) |
| 4 | Internal error |

## Output

- `--format human` (default): a summary and the first 20 diagnostics, each
  with its value and suggested fix. Errors that stop the run go to stderr.
- `--format json`: the [report document](../protocol/README.md#report) for
  `generate` and `validate`, the plan for `explain`. Errors that stop the run
  are printed to stdout as
  `{"schemaVersion":"1.0","status":"error","error":{"kind":"...","message":"..."}}`,
  where `kind` is one of `config`, `io`, `internal`, `delta-refused`, `busy`
  or `state-changed` (`EngineError::kind` in the Rust API). A new kind may
  be added; an existing one keeps its meaning and exit code.

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

A `benchmark` command is planned. It is not present in the binary; the
measurements in [docs/performance](../performance/README.md) use the
Criterion benchmarks (`cargo bench`) and the `make_dataset` example.
