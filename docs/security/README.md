# Security model

## Guarantees of the current engine

| Concern | Measure |
|---|---|
| Code execution | Configuration files are data only. Nothing in them is evaluated or executed. There is no plugin loading |
| `unsafe` | Forbidden in the whole workspace (`unsafe_code = "forbid"`) |
| Malformed XML output | Text is escaped, and characters forbidden by XML 1.0 are refused. URLs are emitted in their WHATWG ASCII form |
| Path traversal in outputs | Artifact names are restricted to `[A-Za-z0-9._-]`, at most 128 characters, with no leading dot. A name can never contain a separator or `..` |
| Partial publication | Writes go to a staging directory. Nothing is published if the run fails. Published files are never deleted |
| Oversized input | JSONL lines are bounded (1 MiB by default), JSON documents are bounded (256 MiB), and URLs are shorter than 2,048 characters |
| Memory | Streaming memory is constant except for the deduplication set (about 40 bytes per URL), which can be disabled |
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

## To do before 1.0

- Fuzz targets (`cargo fuzz`) for the W3C Datetime parser, the hreflang
  parser, the JSONL reader and XML escaping. A property-style test already
  covers escaping round trips.
- `cargo audit` and `cargo deny` in CI (the workflow is prepared but has not
  run yet).
