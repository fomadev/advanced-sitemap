# Security model

## Guarantees of the current engine

| Concern | Measure |
|---|---|
| Code execution | Configuration files are data only. Nothing in them is evaluated or executed. There is no plugin loading |
| `unsafe` | Forbidden in the whole workspace (`unsafe_code = "forbid"`) |
| Malformed XML output | Text is escaped. Characters forbidden by XML 1.0 are reported by validation (`NEWS_TEXT_INVALID`, `VIDEO_TEXT_INVALID`, `VIDEO_UPLOADER_INVALID`) and refused by the writer, with one shared rule. URLs are emitted in their WHATWG ASCII form |
| Path traversal in outputs | Artifact names are restricted to `[A-Za-z0-9._-]`, at most 128 characters, with no leading dot. A name can never contain a separator or `..` |
| Partial publication | Writes go to a staging directory. Nothing is published if the run fails. Published files are never deleted |
| Oversized input | JSONL lines are bounded (1 MiB by default), JSON documents are bounded (256 MiB), and URLs are shorter than 2,048 characters |
| Memory | Streaming memory is constant except for the deduplication set (17 to 54 bytes per URL, measured), which can be disabled; see [limits](../concepts/limits.md) |
| Clock and environment | Never read by the engine |
| Network | The engine makes no network access in this version |

### Reading untrusted sitemaps (`inspect`, `sitemap-reader`)

- No DTD is processed: a `DOCTYPE` stops reading with
  `XML_DOCTYPE_FORBIDDEN`. External entities (XXE) and recursive entity
  expansion ("billion laughs") are therefore impossible. Only the five
  predefined entities and character references are decoded.
- The uncompressed size is capped (50 MiB by default) by a counting reader
  placed after gzip decompression, so a small gzip bomb stops after reading
  at most the limit plus one byte. A test decompresses 64 MiB of input
  under a 4 MiB limit.
- Nesting depth (32) and the text of one element (64 KiB) are capped.
- Only UTF-8 is accepted. Characters forbidden by XML 1.0 stop reading.
- Memory is bounded by the limits above plus 16 bytes per distinct URL
  when duplicate detection is on (at most 50,000 entries per file).
- Errors of the caller's reader are returned as I/O errors and never
  confused with corrupt data.

### State directory (`state.directory`)

- It holds `<stem>.snapshot`: every published URL, its content digest and
  its partition. The URLs are public, but the file is an internal state:
  keep the directory outside the served output directory.
- The snapshot is checked (count and BLAKE3 checksum) before it is trusted;
  a damaged or foreign file is ignored and replaced (`SNAPSHOT_INVALID`).

### Concurrent runs and interrupted runs

- Every writing run locks its output directory and its state directory
  (operating system advisory locks, released on process exit). This
  prevents two runs from interleaving their publications or an `apply`
  from losing another run's changes. Programs other than Advanced Sitemap
  are not excluded.
- Data is synced before each rename, and on Unix the directory is synced
  after the renames. Resistance to a power cut is not tested.
- See [docs/production/audit.md](../production/audit.md) for the exact
  guarantees and what remains open.

## Trust boundaries

- The **CLI** trusts its operator. Input and output paths, including
  `source.path` and `output.directory` in a configuration file, may point
  anywhere the user can read or write. Do not run the CLI on configuration
  files from untrusted parties without reviewing those paths.
- A future **HTTP service** must not accept file paths from clients. It
  should accept records in the request body, write only to a store it
  controls, and enforce authentication, request size limits, timeouts and
  per-tenant isolation.
- Future **network features** (IndexNow, sitemap inspection, health checks)
  must reuse an SSRF guard equivalent to the one already used in the
  hosted Sitemap Cloud backend. That guard resolves DNS, refuses
  private, loopback and link-local addresses at every redirect, bounds
  redirects, applies timeouts, and limits response sizes, including after
  gzip decompression.
- Future **plugins** will run as compiled-in Rust code (trusted) or behind
  an isolation boundary (WASM or a separate process speaking the JSON
  protocol). An unstable Rust ABI will not be exposed.

## Fuzzing and supply chain

- Reproducible fuzzing on stable Rust runs with the tests
  (`tests/fuzz.rs`, `FUZZ_ITERATIONS`, `FUZZ_SEED`) and longer in the CI job
  `fuzz-stable`; coverage-guided targets for `cargo fuzz` are in `fuzz/`
  and run weekly. Results and the defect found are in
  [docs/production/security-report.md](../production/security-report.md).
- `cargo deny` checks advisories (RustSec, the database of `cargo audit`),
  licenses, duplicate versions and sources on every pull request. GitHub
  Actions are pinned by commit hash and kept current by Dependabot.
