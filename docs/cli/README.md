# CLI: `advanced-sitemap`

```
advanced-sitemap [--format human|json] <generate|validate|explain> [options]
```

| Command | Effect |
|---|---|
| `generate` | Validates the input and publishes sitemap files into the output directory |
| `validate` | Runs the same pipeline with a store that keeps nothing; writes no file |
| `explain` | Samples the input and describes what `generate` would do; writes no file |

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
| `--max-diagnostics N` | `validation.maxDiagnostics` |
| `--strategy sequential\|stable-hash` | `partitioning.strategy` |
| `--partitions N` | `partitioning.partitions` |
| `--max-urls N` | `partitioning.maxUrls` |
| `--max-bytes N` | `partitioning.maxBytes` |
| `--index auto\|always\|never` | `output.index` |
| `--file-stem NAME` | `output.fileStem` |
| `--no-manifest` (`generate` only) | `output.manifest = false` |
| `--deny-warnings` (`generate`, `validate`) | exit 1 if any warning or error is reported |
| `--sample N` (`explain` only) | number of records sampled, default 1000 |

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
