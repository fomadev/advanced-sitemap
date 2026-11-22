# Performance

These are measurements, not guarantees. They were taken on one development
laptop under normal desktop load. The wide confidence intervals reflect that
noise. Re-run the protocol below on the target hardware before relying on any
number.

## Environment (2026-11-21)

| Item | Value |
|---|---|
| CPU | Intel Core i5-6300U @ 2.40 GHz (2 cores, 4 threads, 2015) |
| Memory | 11.9 GB |
| OS | Windows 11 Pro |
| Rust | 1.99.0, `release` profile (`bench` profile for Criterion) |
| Storage | Input and output on drive E:, which is known to be slow |
| Engine | 0.1.0, single-threaded |

## End-to-end CLI runs

Dataset: `make_dataset` (deterministic). Every URL has a `lastmod`, and one
in ten has two hreflang alternates. Each record is about 127 bytes of JSONL.
Wall time is measured with `Stopwatch`. Peak working set is sampled every
50 ms from `Process.PeakWorkingSet64`.

| Command | URLs | Wall time | Throughput | Peak working set | Output |
|---|---|---|---|---|---|
| `validate` | 1,000,000 | 7.1 s | ~142 k URL/s | 56 MiB | — |
| `validate --no-dedupe` | 1,000,000 | 5.0 s | ~200 k URL/s | 5 MiB | — |
| `generate` | 1,000,000 | 16.3 s | ~62 k URL/s | 57 MiB | 20 files, 168.6 MB |
| `validate` | 5,000,000 | 23.6 s | ~212 k URL/s | 210 MiB | — |
| `validate --no-dedupe` | 5,000,000 | 31.3 s | ~160 k URL/s | 5 MiB | — |
| `generate` | 5,000,000 | 80.4 s | ~62 k URL/s | 209 MiB | 100 files, 848.1 MB, max 50,000 URLs and 8.5 MB per file |

Observations:

- **Memory without deduplication is constant**: 5 MiB at both 1 M and 5 M
  URLs. With deduplication, memory grows with the number of distinct URLs.
  This is the only cost that grows with input size. See
  [Deduplication memory](#deduplication-memory) below.
- **The two modes do not produce the same output.** `--no-dedupe` turns off
  a correctness check: duplicate `loc` values are then written twice, which
  search engines report as an error. Its numbers show the cost of the
  pipeline without the check. They are not a faster way to do the same work.
- **`generate` is limited by disk writes**, not by computation. `validate`
  runs the same pipeline into a null store and is 2.3 to 3.4 times faster.
  The gap is writing about 850 MB to the slow E: drive, plus one `fsync` per
  file.
- Throughput varies by about 1.5x between identical runs, partly because
  the first `validate` at 1 M read the input from a cold cache. Treat these
  numbers as orders of magnitude.

## 10 million URLs (lot C, 2026-11-22)

Same dataset generator (`make_dataset`, 1.28 GB of JSONL), same machine,
measured in one series:

| Command | Wall time | Throughput | Peak working set | Output |
|---|---|---|---|---|
| `validate` | 54.4 s | ~184 k URL/s | 416 MiB | — |
| `validate --no-dedupe` | 29.6 s | ~338 k URL/s | 5 MiB | — |
| `generate` | 80.8 s | ~124 k URL/s | 415 MiB | 203 files, 1.70 GB |
| Rerun, `--execution-mode auto` (nothing changed) | 58.9 s | | 414 MiB | 0 files rewritten |
| Rerun with `--state-dir`, first snapshot | 76.6 s | | 461 MiB | snapshot 448 MB |
| Rerun with `--state-dir`, nothing changed | 80.4 s | | 461 MiB | 10,000,000 unchanged URLs |
| `explain` (sample of 1,000 records) | 3.7 s | | 2 MiB | estimated 10.39 M records (+3.9 %) |

- Memory without deduplication stays at 5 MiB, as at 1 M and 5 M.
- With deduplication the peak is 411 MiB of table for 10 M URLs, the
  transient peak predicted by the model below (2^24 slots).
- The snapshot adds 46 MiB of memory only (sort budget), and 1.8 to 2.2
  microseconds per URL.
- These runs are faster per URL than the 5 M runs of 2026-11-21 (80.4 s
  for 5 M): the same slow disk varies by up to three times between days.
  Compare runs of one series only.
- 50 M URLs were not run: the disk of the reference machine has 10 GB
  free; the expected costs are 2.2 GB of snapshot, about 1.6 GiB of
  deduplication table at its peak, and 8.5 GB of output.

## Allocation profile

`tools/alloc-profile` (outside the workspace, because it needs a counting
global allocator) counts heap allocations per URL, on 200,000 URLs in
memory:

| Stage | Allocations per URL | Bytes allocated per URL | Share of time |
|---|---|---|---|
| Decoding a JSONL record | 2.5 | 115 | about 20 % |
| Validation | 3.2 | 145 | about 45 %, two thirds of it in URL parsing (`url` crate, WHATWG rules) |
| Encoding the `<url>` block | 0 | 0 | about 12 % (buffer reused) |
| Whole `validate` pipeline | 5.7 | 305 | 3.8 microseconds per URL |
| `generate` with a snapshot | 7.7 | 938 | |
| A change-feed operation held by `apply` | — | 806 bytes live per operation | |

No allocation was removed in lot C: the remaining ones are the decoded
strings and the parsed URL, each needed once. The largest CPU cost is URL
parsing, required for correctness. The only large gain left would be to
validate on several threads (the machine has two cores), with an ordered
merge to keep the output deterministic; not done, see the risk register.

## Republishing only changed files

`generate --skip-unchanged` (`output.publish = "changed"`) compares each new
file with the published one while writing it and writes nothing while the
bytes are identical. Same dataset, same machine, output already published:

| Run | 1 M URLs (22 files) | 5 M URLs (102 files) |
|---|---|---|
| `generate` (republish all) | 10.6 to 12.7 s | 44.1 s (69 to 123 s in earlier runs) |
| `generate --skip-unchanged`, nothing changed | 5.4 to 5.6 s | 22.4 to 22.8 s |
| `validate` (no output at all), for reference | 4.2 to 7.1 s | 23.6 s |
| `--skip-unchanged`, one `lastmod` changed | 5.8 s, 2 files republished | not measured |

With nothing to republish, `generate` costs what `validate` costs: reading,
validating and encoding. The disk of this machine is noisy, as the spread
of the full runs shows.

How the design got there, measured at 1 M URLs: comparing after writing
the staging file saved nothing (12.2 s against 12.4 s). Moving the `fsync`
from the end of each staging write to just before its rename brought the
unchanged rerun to 4.5 s at 1 M, but not at 5 M (74.8 s), where writing
848 MB of staging files still dominated. Comparing while writing removed
that cost.

## Inspecting existing files

`inspect` on the 20 files produced by `generate` for 1 M URLs (168.6 MB):
9.5 to 9.8 s once the files are in the OS cache (12.9 s on the first, cold
run), about 100 k URL/s or 17 MB/s, single-threaded. Peak working set while
inspecting one 50,000-URL file: 4.8 MiB. All 1,000,000 entries were read
back as valid.

## URL-level snapshot (`--state-dir`)

Measured with `generate --skip-unchanged` on the 1 M URL dataset, output
already published, and `validate` at 5 M:

| Run | Without snapshot | With snapshot |
|---|---|---|
| 1 M, rerun without changes | 3.3 s, 57 MiB (3.9 to 4.7 s before) | 6.3 to 6.7 s, 111 MiB (8.6 to 10.3 s before) |
| 5 M, `validate` (snapshot built and compared, not kept) | 18.4 s, 210 MiB | 35.5 s, 246 MiB |

- The snapshot takes 44.8 MB at 1 M URLs (about 45 bytes per URL, after
  prefix compression of the sorted URLs; 86.7 MB without it, and 47.4 s at
  5 M).
- Memory is bounded by the sort budget (64 MiB by default): at 5 M URLs
  the peak grows by 36 MiB only, because sorted runs are spilled to the
  state directory (invariant I8).
- The extra time fell from 4 to 5.5 s to 3.0 to 3.4 s at 1 M URLs once
  the snapshot was hashed and copied in 64 KiB blocks instead of record by
  record (same format). Per stage, on 1 M records
  (`cargo run --release -p sitemap-snapshot --example snapshot_stages`):

  | Stage | Before | After |
  |---|---|---|
  | Read a snapshot | 0.61 to 0.70 s | 0.26 s |
  | Diff two snapshots | 0.99 to 1.87 s | 0.54 to 0.56 s |
  | Push, sort and spill | 0.6 to 0.8 s | unchanged |
  | Merge and write | 0.85 to 0.94 s | unchanged |
  | Sync before the rename | 1.1 to 1.7 s | unchanged |

- What remains is mostly the sync of the new snapshot (32 to 45 MB on this
  slow drive) and the sort. The sync is kept: without it, a crash right
  after a publication could leave an older, valid snapshot, which makes
  the next change report inaccurate and the next `apply` refuse. A
  damaged snapshot is always detected by its checksum. The option is off
  by default.

## Comparing two published sets (`diff`)

Two `generate` outputs of 1 M URLs (21 files and 168.6 MB each), differing
by one `lastmod`: `diff` found exactly 1 modified URL and 999,999 unchanged.
46.9 s on the first run (files not in the OS cache), 26.2 s on the second;
`inspect` alone on the same two directories takes 19 to 25 s, so reading
and validating dominate. Peak working set: 94 MiB.

## `consistent-hash` and migration cost

1 M URL dataset, 24 partitions, `validate` with a warm cache:

| Strategy | Time | Peak | URLs per file (min to max) |
|---|---|---|---|
| `stable-hash` | 3.7 s | 57 MiB | 41,383 to 42,004 |
| `consistent-hash` | 3.5 to 3.8 s | 57 MiB | 41,136 to 42,029 |

Growing from 24 to 25 partitions after a `generate` with `--state-dir`:

| | `consistent-hash` 25 | `stable-hash` 25 |
|---|---|---|
| URLs changing file, predicted by `explain` | 40,112 (4.0 %; 1/25 = 4 %) | 959,996 (96.0 %) |
| `explain` time with the 1 M URL snapshot | 0.7 to 0.8 s, 5 MiB | 0.7 s, 5 MiB |

The following `generate --strategy consistent-hash --partitions 25`
reported exactly 40,112 moved URLs, as predicted. It still rewrote all 27
files (25 sitemaps, index, manifest): every old partition gave about 4 % of
its URLs to the new one. The gain is in URL placement (most URLs stay in
the same file for crawlers, and the change report stays small), not in
fewer files written on that run.

## Delta mode (`apply`)

1 M URL dataset sorted by URL, `consistent-hash`. Feeds of random URLs, 80 %
upserts and 20 % deletes, applied one after the other:

| | 24 partitions (50,000 URLs max) | 1,000 partitions (2,000 URLs max) |
|---|---|---|
| Full `generate` | 16.5 to 16.8 s, 117 MiB | 37.5 to 41.3 s, 287 MiB |
| `apply`, 1 operation | 3.3 to 3.4 s, 50 MiB (4.9 to 6.5 s, 95 MiB before) | 4.6 to 5.4 s, 94 MiB (measured before) |
| `apply`, 100 operations | 24.8 s, 126 MiB (all 24 partitions touched) | 9.2 to 11.1 s, 95 MiB |
| `apply`, 10,000 operations | 20.5 s, 133 MiB | 49.6 to 54.2 s, 117 MiB |

- **When it pays:** few partitions touched. Each touched partition is read
  back, checked and rewritten, which costs more per URL than generating it
  from JSON Lines; a feed that touches every partition is slower than a
  full `generate` (use `generate --skip-unchanged` then). Many small
  partitions make feeds touch a smaller share of the site.
- **Fixed cost:** about 3.3 s at 1 M URLs (4.5 s before), almost all
  snapshot I/O. The unchanged records are now merged from the previous
  snapshot, already sorted, instead of being read a second time, sorted
  and spilled again. The 100-operation run at 24 partitions went from
  24.8 s to 21.1 s.
- **Checked at this scale:** after the three feeds (997,988 URLs left), the
  26 published files were byte for byte those of a full `generate` from
  the same final URLs.
- Memory: checking a touched partition uses one digest per partition, and
  each partition is closed once rewritten. Before these two changes, the
  100- and 10,000-operation runs peaked at 293 to 300 MiB (24 partitions)
  and 273 MiB (1,000 partitions).

## Execution planner estimates

The planner's cost model (`CostModel::REFERENCE`) was recalibrated in lot C
on one series of runs made under the same disk conditions (the first
calibration mixed days when the disk was up to three times slower, and
overestimated full generations twice at 10 M URLs). Estimates against
measurements:

| Run | Estimated | Measured |
|---|---|---|
| `generate` 1 M, 20 files | 9.9 s | 10.3 and 12.7 s |
| `generate` 1 M, 1,000 files | 42 s | 43.1 and 46.2 s |
| `generate` 10 M, 203 files (records estimated from a sample) | 102 s | 80.8 s |
| Rerun 10 M without changes, no snapshot / with snapshot | 52 / 83 s | 58.9 / 76.6 to 80.4 s |
| Peak memory 10 M, no snapshot / with snapshot | 499 / 566 MB | 434 / 483 MB |
| `apply` 1 operation (24 / 1,000 partitions, lot B) | 3.8 / 3.1 s | 3.4 to 5.5 s |
| `apply` 100 operations, 24 / 1,000 partitions | 21.0 / 7.7 s | 22.0 to 25.0 / 7.3 to 9.5 s |
| `apply` 10,000 operations, 24 / 1,000 partitions | 21.0 / 53 s | 21.4 to 22.7 / 42.2 to 44.5 s |

- Identical runs differ by up to 20 % on this disk; most estimates fall
  inside that noise. Full generations of 10 M URLs are still
  overestimated (26 %: writing gets cheaper per byte on long runs), and
  memory by 15 to 17 % (on the safe side).
- The planner's decisions do not change: for 100 operations over 24
  partitions and 10,000 over 1,000, it recommends regenerating; a full
  generation from the source took 15 to 17 s and 37 to 40 s against 21 to
  25 s and 42 to 45 s for `apply`.
- Lot B costs nothing measurable: same commands with the binary from
  before lot B, four alternated runs each, median 7.1 s for both on the
  unchanged rerun and 3.6 s against 3.7 s for `apply` with one operation.

## hreflang reciprocity

`validate` on the 1 M URL dataset (100,000 entries with 2 alternates, so
200,000 links): 56.5 MiB and 4.2 s without the check, 77.8 MiB and 5.2 s
with `--hreflang-reciprocity`. The extra 21 MiB is about 110 bytes per
link: the edge list plus the pair map built at the end. With
deduplication on, the set of written URLs is shared with the
deduplication set; without it, the check keeps its own (see below for its
size).

## Deduplication memory

Deduplication keeps one 128-bit fingerprint per distinct `loc` in a
`std::collections::HashSet<u128>` (hashbrown). It never stores the URL
itself. The memory follows from how that table works:

- each slot costs 16 bytes of fingerprint plus 1 control byte;
- the table holds at most 7/8 of its slots and grows by doubling, so the
  slot count is the next power of two above `n / 0.875`;
- while growing, the old and the new tables exist at the same time.

| Distinct URLs | Slots | Table after growth | Transient peak while growing |
|---|---|---|---|
| 1,000,000 | 2^21 | 34 MiB | 51 MiB |
| 5,000,000 | 2^23 | 136 MiB | 204 MiB |
| 10,000,000 | 2^24 | 272 MiB | 408 MiB |
| 50,000,000 | 2^26 | 1.06 GiB | 1.6 GiB |

The measured peaks (56 MiB at 1 M, 210 MiB at 5 M, 416 MiB at 10 M,
including about 5 MiB for the rest of the process) match this model. Steady state is 17 to 34
bytes per distinct URL depending on where `n` falls between two powers of
two. The peak while growing is 1.5 times the table.

Trade-offs, in order of preference:

1. **Keep deduplication on** (the default). It is exact for fingerprints:
   two different URLs collide with a probability around n²/2¹²⁹, which is
   about 10⁻²³ at 50 M URLs.
2. **Turn it off only when the source guarantees unique `loc` values**, for
   example a primary key in a database. The memory then stays constant.
3. Not implemented, candidates if a user needs more than about 100 M URLs
   per run: pre-size the table from an expected count (removes the growth
   peak), or deduplicate on disk with an external sort (constant memory,
   extra I/O). Neither will be added without a measured need.

## Criterion micro-benchmarks

`cargo bench -p advanced-sitemap --bench pipeline` uses in-memory records and
a null store. The record clone in each iteration is included in the
`pipeline/*` measurements. Values are [low, estimate, high]:

| Benchmark | Time | Throughput (estimate) |
|---|---|---|
| pipeline, 1,000 URLs | [10.7, 12.2, 13.9] ms | 82 k URL/s |
| pipeline, 10,000 URLs | [29.2, 45.3, 60.5] ms | 221 k URL/s |
| pipeline, 100,000 URLs | [364, 443, 531] ms | 226 k URL/s |
| strategy, sequential, 100 k | [243, 344, 493] ms | 291 k URL/s |
| strategy, stable-hash with 8 partitions, 100 k | [336, 407, 487] ms | 246 k URL/s |
| XML encoding only, 100 k | [54.7, 63.6, 74.0] ms | 1.57 M URL/s |
| validation only, 100 k | [163, 186, 214] ms | 538 k URL/s |

Reading: in the full pipeline, validation (WHATWG URL parsing, W3C date
parsing, hreflang) accounts for roughly 40% of the CPU time and XML encoding
for roughly 15%. The rest is input cloning, deduplication hashing and
routing. The 1,000-URL case has a per-run fixed cost that has not been
profiled yet.

## No optimization yet

Following the project rules, nothing has been optimized. Candidates, to be
justified with profiles first: reuse URL parsing buffers, parse records
from JSONL without an intermediate `String`, and run validation in parallel
in front of the sequential writer.

## Reproducing

```sh
cargo build --release -p sitemap-cli
cargo run --release -p advanced-sitemap --example make_dataset -- 1000000 bench-output/1000000.jsonl
target/release/advanced-sitemap --format json validate --input bench-output/1000000.jsonl --public-base-url https://shop.example.com/
target/release/advanced-sitemap --format json generate --input bench-output/1000000.jsonl --public-base-url https://shop.example.com/ --out bench-output/out
cargo bench -p advanced-sitemap --bench pipeline
```

`bench-output/` is ignored by git.
