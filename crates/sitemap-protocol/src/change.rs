//! Change feeds for the `delta` mode (ADR-0013).
//!
//! JSON Lines, one operation per line, applied in order (the last operation
//! on a URL wins):
//!
//! ```json
//! {"op": "upsert", "url": {"loc": "https://example.com/a", "lastmod": "2026-11-22"}}
//! {"op": "delete", "loc": "https://example.com/b"}
//! ```

use serde::{Deserialize, Serialize};
use sitemap_model::RawUrlRecord;

/// One operation of a change feed.
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(tag = "op", rename_all = "kebab-case", deny_unknown_fields)]
// Records are decoded and consumed one at a time; boxing would only add an
// allocation per operation.
#[allow(clippy::large_enum_variant)]
#[non_exhaustive]
pub enum ChangeRecord {
    /// Adds the URL, or replaces every field of an existing one.
    Upsert {
        /// The complete URL record (same schema as a source record).
        url: RawUrlRecord,
    },
    /// Removes the URL. Removing an unknown URL changes nothing.
    Delete {
        /// URL to remove.
        loc: String,
    },
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::ChangeRecord;
    use crate::JsonlReader;

    #[test]
    fn reads_a_change_feed() {
        let feed = b"{\"op\":\"upsert\",\"url\":{\"loc\":\"https://example.com/a\",\"lastmod\":\"2026-11-22\"}}\n\n{\"op\":\"delete\",\"loc\":\"https://example.com/b\"}\n{\"op\":\"rename\"}\n{\"op\":\"delete\",\"loc\":\"x\",\"extra\":1}\n";
        let items: Vec<_> = JsonlReader::<_, ChangeRecord>::typed(&feed[..], 1024)
            .map(Result::unwrap)
            .collect();
        assert_eq!(items.len(), 4);
        match &items[0].result {
            Ok(ChangeRecord::Upsert { url }) => {
                assert_eq!(url.loc.as_deref(), Some("https://example.com/a"));
            }
            other => panic!("{other:?}"),
        }
        assert_eq!(
            items[1].result,
            Ok(ChangeRecord::Delete {
                loc: "https://example.com/b".to_owned()
            })
        );
        assert_eq!(items[1].line, Some(3));
        assert!(items[2].result.is_err(), "unknown op");
        assert!(items[3].result.is_err(), "unknown field");
    }
}
