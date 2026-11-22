//! ADR-0013 prerequisite, as a property test: for any valid entry,
//! `encode(read(encode(e))) == encode(e)`. Entries are generated at random
//! (deterministic seed) with every field and every extension, including
//! characters that must be escaped and non-ASCII text.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, missing_docs)]

use advanced_sitemap::inspect::inspect_reader;
use advanced_sitemap::{
    InspectOptions, Item, Location, Namespaces, RawUrlRecord, ValidationOptions, Validator, xml,
};
use serde_json::{Value, json};

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }

    fn chance(&mut self, percent: u64) -> bool {
        self.below(100) < percent
    }

    fn pick<'a>(&mut self, items: &[&'a str]) -> &'a str {
        items[self.below(items.len() as u64) as usize]
    }

    /// Text with markup characters, quotes, non-ASCII letters and inner spaces.
    fn text(&mut self, max_words: u64) -> String {
        const WORDS: &[&str] = &[
            "plain",
            "a&b",
            "<tag>",
            "\"quoted\"",
            "it's",
            "café",
            "日本語",
            "Ünïcödé",
            "50%",
            "x>y",
            "emoji\u{1F600}",
            "tab",
            "Ωmega",
            "&amp;",
            "]]>",
        ];
        let words = 1 + self.below(max_words);
        let text = (0..words)
            .map(|_| self.pick(WORDS))
            .collect::<Vec<_>>()
            .join(self.pick(&[
                " ", " ", "  ", "
", "	",
            ]));
        // Surrounding whitespace, which validation must normalize the same
        // way the reader does.
        match self.below(6) {
            0 => format!("  {text}"),
            1 => format!(
                "{text} 
"
            ),
            _ => text,
        }
    }

    fn url(&mut self, id: u64, kind: &str) -> String {
        let path = self.pick(&["page", "caf%C3%A9", "a-b_c.html", "x/y/z", "%E6%97%A5"]);
        let query = self.pick(&["", "?a=1", "?a=1&b=2", "?q=%3Ctag%3E", "?x=it's"]);
        format!("https://example.com/{kind}/{id}/{path}{query}")
    }

    fn date(&mut self) -> String {
        self.pick(&[
            "2026-01-02",
            "2026-01-02T03:04:05Z",
            "2026-01-02T03:04:05+02:00",
            "2026-11-21T23:59:59-05:30",
            "2026-01-02T03:04:05.123Z",
            "2026-01-02T03:04Z",
        ])
        .to_owned()
    }
}

fn random_record(rng: &mut Rng, id: u64) -> Value {
    let mut record = json!({ "loc": rng.url(id, "p") });
    if rng.chance(60) {
        record["lastmod"] = json!(rng.date());
    }
    if rng.chance(40) {
        record["changefreq"] = json!(rng.pick(&[
            "always", "hourly", "daily", "weekly", "monthly", "yearly", "never"
        ]));
    }
    if rng.chance(40) {
        record["priority"] = json!([0.0, 0.1, 0.5, 0.25, 0.8, 1.0, 0.333][rng.below(7) as usize]);
    }
    if rng.chance(40) {
        let langs = ["en", "fr", "de-CH", "zh-Hant", "pt-BR", "x-default"];
        let count = 1 + rng.below(4) as usize;
        record["alternates"] = json!(
            langs[..count]
                .iter()
                .map(|lang| json!({"hreflang": lang, "href": rng.url(id, lang)}))
                .collect::<Vec<_>>()
        );
    }
    if rng.chance(40) {
        record["images"] = json!(
            (0..1 + rng.below(3))
                .map(|i| json!({"loc": rng.url(id * 10 + i, "img")}))
                .collect::<Vec<_>>()
        );
    }
    if rng.chance(30) {
        let mut video = json!({
            "thumbnailLoc": rng.url(id, "thumb"),
            "title": rng.text(6),
            "description": rng.text(20),
        });
        if rng.chance(50) {
            video["contentLoc"] = json!(rng.url(id, "video"));
        } else {
            video["playerLoc"] = json!(rng.url(id, "player"));
        }
        if rng.chance(50) {
            video["duration"] = json!(1 + rng.below(28_800));
        }
        if rng.chance(30) {
            video["expirationDate"] = json!(rng.date());
        }
        if rng.chance(40) {
            video["rating"] = json!([0.0, 2.5, 4.2, 5.0, 3.7][rng.below(5) as usize]);
        }
        if rng.chance(40) {
            video["viewCount"] = json!(rng.below(1_000_000_000));
        }
        if rng.chance(40) {
            video["publicationDate"] = json!(rng.date());
        }
        if rng.chance(40) {
            video["tags"] = json!(
                (0..1 + rng.below(4))
                    .map(|_| rng.text(2))
                    .collect::<Vec<_>>()
            );
        }
        if rng.chance(30) {
            video["familyFriendly"] = json!(rng.chance(50));
        }
        if rng.chance(30) {
            video["restriction"] = json!({
                "relationship": rng.pick(&["allow", "deny"]),
                "countries": (["FR", "US", "JP"][..1 + rng.below(3) as usize]).to_vec(),
            });
        }
        if rng.chance(30) {
            video["platform"] = json!({
                "relationship": rng.pick(&["allow", "deny"]),
                "platforms": (["web", "mobile", "tv"][..1 + rng.below(3) as usize]).to_vec(),
            });
        }
        if rng.chance(30) {
            video["requiresSubscription"] = json!(rng.chance(50));
        }
        if rng.chance(30) {
            let mut uploader = json!({"name": rng.text(3)});
            if rng.chance(50) {
                uploader["info"] = json!(rng.url(id, "user"));
            }
            video["uploader"] = uploader;
        }
        if rng.chance(30) {
            video["live"] = json!(rng.chance(50));
        }
        record["videos"] = json!([video]);
    }
    if rng.chance(25) {
        record["news"] = json!({
            "publication": {"name": rng.text(3), "language": rng.pick(&["en", "fr", "zh-cn", "zh-tw", "ast"])},
            "publicationDate": rng.date(),
            "title": rng.text(8),
        });
    }
    record
}

#[test]
fn reading_and_reencoding_random_entries_is_exact() {
    let validator = Validator::new(ValidationOptions::default());
    let mut rng = Rng(0x0dd_ba11);
    let mut scratch = Vec::new();
    let mut expected = Vec::new();
    let mut valid = 0;
    let mut with = [0u32; 4]; // alternates, images, videos, news
    for id in 0..3_000 {
        let value = random_record(&mut rng, id);
        let raw: RawUrlRecord = serde_json::from_value(value.clone()).unwrap();
        scratch.clear();
        let Some(entry) = validator.validate(&raw, &Location::record(id, None), &mut scratch)
        else {
            panic!("generated record {id} is invalid: {scratch:?}\n{value}");
        };
        with[0] += u32::from(!entry.alternates.is_empty());
        with[1] += u32::from(!entry.images.is_empty());
        with[2] += u32::from(!entry.videos.is_empty());
        with[3] += u32::from(entry.news.is_some());
        xml::encode_url(&entry, &mut expected).unwrap();
        valid += 1;
    }
    assert!(with.iter().all(|n| *n > 300), "{with:?}");

    let mut document = xml::urlset_header(Namespaces::ALL).into_bytes();
    document.extend_from_slice(&expected);
    document.extend_from_slice(xml::URLSET_FOOTER.as_bytes());

    let mut again = Vec::new();
    let report = inspect_reader(
        "random",
        &document[..],
        &InspectOptions::default(),
        |item| {
            if let Item::Url(entry) = item {
                xml::encode_url(&entry, &mut again).unwrap();
            }
        },
    )
    .unwrap();
    assert!(report.is_valid(), "{:?}", report.diagnostics);
    assert_eq!(report.valid_entries, valid);

    // Compare block by block for a readable failure.
    let split = |bytes: &[u8]| -> Vec<String> {
        String::from_utf8(bytes.to_vec())
            .unwrap()
            .split_inclusive("</url>\n")
            .map(str::to_owned)
            .collect()
    };
    let (expected, again) = (split(&expected), split(&again));
    assert_eq!(expected.len(), again.len());
    for (i, (a, b)) in expected.iter().zip(&again).enumerate() {
        assert_eq!(a, b, "entry {i} differs after a round trip");
    }
}
