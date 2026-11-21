# Diagnostic codes

Codes are a stable contract (ADR-0003). Adding a code is a minor change.
Renaming or removing one is a breaking change. The severity shown is the
`standard` profile. **(strict)** marks rules that the `strict` profile
promotes to `error`, or that run only in that profile.

Severities: `error` blocks the record or the run. `warning` means the data is
probably wrong but the output is valid. `recommendation` means a best
practice is not followed. `info` is informational.

## Input

| Code | Severity | Meaning |
|---|---|---|
| `INPUT_PARSE_ERROR` | error | A record is not valid JSON, has a field of the wrong type, or has an unknown field |
| `INPUT_LINE_TOO_LONG` | error | A JSONL line exceeds `maxLineBytes`. It is skipped and reading continues |

## URLs

| Code | Severity | Meaning |
|---|---|---|
| `REQUIRED_FIELD_MISSING` | error | `loc`, `hreflang`, `href` or an image `loc` is absent (see `location.field`) |
| `URL_INVALID` | error | Whitespace, a control character, or a WHATWG parse failure. The engine never trims or repairs |
| `URL_UNSUPPORTED_SCHEME` | error | Not an absolute `http://` or `https://` URL |
| `URL_TOO_LONG` | error | 2,048 characters or more after serialization |
| `URL_HAS_CREDENTIALS` | error | Contains `user:password@` |
| `URL_HAS_FRAGMENT` | warning | Has `#...`, which search engines ignore |
| `URL_NOT_HTTPS` | recommendation (strict: error) | Uses `http` |
| `HOST_MISMATCH` | warning (strict: error) | Origin differs from `site` |
| `DUPLICATE_URL` | warning | Already listed. This occurrence is dropped |

## Fields

| Code | Severity | Meaning |
|---|---|---|
| `LASTMOD_INVALID` | error | Not a W3C Datetime. A time requires a zone (`Z` or `±hh:mm`) |
| `LASTMOD_MISSING` | error (strict only) | `lastmod` absent |
| `LASTMOD_IN_FUTURE` | warning | Later than `referenceTime`. Runs only when `referenceTime` is set |
| `CHANGEFREQ_INVALID` | error | Not one of the lowercase keywords: always, hourly, daily, weekly, monthly, yearly, never |
| `CHANGEFREQ_MISSING` | error (strict only) | `changefreq` absent |
| `PRIORITY_OUT_OF_RANGE` | error | Not between 0.0 and 1.0 |
| `PRIORITY_MISSING` | error (strict only) | `priority` absent |

## Extensions

| Code | Severity | Meaning |
|---|---|---|
| `HREFLANG_INVALID` | error | Not `x-default` or `lang[-Script][-REGION]` (for example `en`, `en-GB`, `zh-Hant`, `es-419`) |
| `HREFLANG_DUPLICATE` | warning | The same language code appears twice for one URL |
| `ALTERNATE_SELF_MISSING` | recommendation | The URL has alternates but does not list itself among them |
| `IMAGE_LIMIT_EXCEEDED` | error | More than 1,000 images for one URL |

## Generation

| Code | Severity | Meaning |
|---|---|---|
| `ENTRY_TOO_LARGE` | error | One entry alone does not fit under `maxBytes` |
| `PARTITION_LIMIT_EXCEEDED` | error | A `stable-hash` partition is full. Always fails the run. Increase `partitions` |
| `INDEX_LIMIT_EXCEEDED` | error | More than 50,000 sitemap files would be referenced |
| `PUBLIC_BASE_URL_MISSING` | error | An index is needed but `publicBaseUrl` is not set |
| `EMPTY_SITEMAP` | warning | No URL was written |
| `STALE_ARTIFACT` | info | A file from the previous manifest is no longer produced. It is not deleted |
| `DIAGNOSTICS_TRUNCATED` | info | The list reached `maxDiagnostics`. Counts are exact |

## Reading existing files (`inspect`)

Codes marked *stops* end reading: the file report then has
`complete: false` and its counts cover only the part read.

| Code | Severity | Meaning |
|---|---|---|
| `XML_MALFORMED` | error | Not well-formed XML (mismatched tags, invalid UTF-8, forbidden character, undefined entity, content after the root). *Stops* |
| `XML_DOCTYPE_FORBIDDEN` | error | A `DOCTYPE` is present. Refused so that no entity can be declared or expanded. *Stops* |
| `XML_ENCODING_UNSUPPORTED` | error | The declared encoding is not UTF-8. *Stops* |
| `XML_ROOT_INVALID` | error | The root is not `urlset` or `sitemapindex` in the sitemaps.org namespace. *Stops* |
| `XML_UNEXPECTED_ELEMENT` | warning | Unknown element of the sitemaps.org namespace, a repeated field, or `xhtml:link` without `rel="alternate"`. Ignored |
| `XML_ELEMENT_ORDER` | warning | Elements are not in the XSD order: `loc`, `lastmod`, `changefreq`, `priority`, then extensions. Reported once per entry |
| `XML_DEPTH_EXCEEDED` | error | Nesting deeper than the limit (32). *Stops* |
| `XML_TEXT_TOO_LONG` | error | The text of one element is longer than the limit (64 KiB). *Stops* |
| `SITEMAP_TOO_LARGE` | error | More than 50 MiB uncompressed (or `--max-bytes`). *Stops* |
| `SITEMAP_URL_LIMIT_EXCEEDED` | error | More than 50,000 `<url>` elements. Reported once; reading continues |
| `GZIP_INVALID` | error | The gzip stream is corrupt. *Stops* |
| `PRIORITY_INVALID` | error | `priority` text is not a number |

An index with more than 50,000 `<sitemap>` elements reports
`INDEX_LIMIT_EXCEEDED`. Entry values are checked with the codes above
(`URL_*`, `LASTMOD_*`, `CHANGEFREQ_INVALID`, `PRIORITY_OUT_OF_RANGE`,
`HREFLANG_*`, `DUPLICATE_URL`, ...); `location.record` is the 1-based
position of the entry in the file.

## Policies

- With `onInvalid: fail` (the default), any `error` prevents publication.
  Validation still continues to the end, so the report lists every problem.
- With `onInvalid: skip`, records with errors are excluded and the rest is
  published. Structural errors (`PARTITION_LIMIT_EXCEEDED`,
  `INDEX_LIMIT_EXCEEDED`, `PUBLIC_BASE_URL_MISSING`) still fail the run.
