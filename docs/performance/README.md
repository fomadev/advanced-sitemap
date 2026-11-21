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
| 50,000,000 | 2^26 | 1.06 GiB | 1.6 GiB |

The measured peaks (56 MiB at 1 M, 210 MiB at 5 M, including about 5 MiB
for the rest of the process) match this model. Steady state is 17 to 34
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
