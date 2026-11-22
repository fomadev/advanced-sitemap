# Contributing

## Before opening a pull request

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace
```

## Rules

- Rust stable, pinned in `rust-toolchain.toml`. `unsafe` is forbidden
  workspace-wide.
- No `unwrap`, `expect` or `panic!` in production code. They are allowed in
  tests.
- Every validation rule has a positive and a negative test. Every new
  diagnostic code is added to `DiagnosticCode`, documented in
  `docs/concepts/diagnostics.md`, and never renamed afterwards.
- Output must stay deterministic. If a change alters generated XML on
  purpose, update `fixtures/conformance/*/expected` in the same commit and
  explain why in the commit message.
- A new dependency needs a justification in
  `docs/architecture/adr/0008-dependencies.md`.
- Performance claims need a reproducible benchmark (`cargo bench`, or the
  protocol in `docs/performance/README.md`).
- Record significant design decisions as an ADR in `docs/architecture/adr/`.

## Licensing of contributions

This project is source-available under the
[Fordima Public License v1.0](LICENSE). As stated in its Section 2, by
opening a Pull Request you agree that your contribution is released under
the same license and may be integrated into the official project. How
contributions may also be included in commercial licenses is still to be
decided (a clause or a contributor agreement); external Pull Requests are
not merged until it is (see
[docs/legal/fpl-review.md](docs/legal/fpl-review.md), section 8).
