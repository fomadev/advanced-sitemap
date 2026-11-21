# Conformance fixtures

Shared by every interface (Rust, CLI, future Node and Python bindings).

Each directory under `conformance/` holds a `config.json` (protocol v1). When
`expected/` exists, a successful run must publish **exactly** those files,
byte for byte (serialization is deterministic, ADR-0006). When
`expected-codes.txt` exists, the run must fail and report exactly those
diagnostic codes, in order.

## How the expected files are trusted

The `expected/` files were produced by the engine and then reviewed by hand.
To avoid checking them only with the code that wrote them, CI also checks
them with tools that share no code with the engine:

- `tools/check_sitemaps.py`: a standard-library Python checker (expat
  parser) that re-implements the sitemaps.org rules: namespace, element
  order, W3C Datetime, `changefreq` values, `priority` range, escaped
  absolute URLs, 50,000 entries and 50 MiB limits, and the shape of the
  `xhtml:link` and `image:image` extensions.
- `xmllint` against the official sitemaps.org XSD, for the fixtures without
  extension namespaces.

Run the Python checker locally with:

```sh
python tools/check_sitemaps.py fixtures/conformance
```
