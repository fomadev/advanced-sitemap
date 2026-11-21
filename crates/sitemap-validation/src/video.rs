//! Validation of the Google video extension.

use std::cmp::Ordering;

use sitemap_model::limits::{
    MAX_VIDEO_DESCRIPTION_CHARS, MAX_VIDEO_DURATION_SECONDS, MAX_VIDEO_TAGS, MAX_VIDEO_TITLE_CHARS,
    MAX_VIDEO_UPLOADER_CHARS,
};
use sitemap_model::{
    CountryCode, Diagnostic, DiagnosticCode, Location, Platform, RawUrlRecord, RawVideo,
    Relationship, SitemapUrl, Video, VideoPlatform, VideoRestriction, VideoUploader, W3cDateTime,
};

use crate::{Validator, missing};

impl Validator {
    pub(crate) fn validate_videos(
        &self,
        raw: &RawUrlRecord,
        page: Option<&SitemapUrl>,
        location: &Location,
        out: &mut Vec<Diagnostic>,
    ) -> Vec<Video> {
        raw.videos
            .iter()
            .enumerate()
            .filter_map(|(i, video)| {
                self.validate_video(video, page, location, &format!("videos[{i}]"), out)
            })
            .collect()
    }

    fn validate_video(
        &self,
        raw: &RawVideo,
        page: Option<&SitemapUrl>,
        location: &Location,
        path: &str,
        out: &mut Vec<Diagnostic>,
    ) -> Option<Video> {
        let first = out.len();
        let field = |name: &str| format!("{path}.{name}");
        let error = |code, message: String, name: &str, value: Option<&str>| {
            let mut diagnostic = Diagnostic::new(code, message, location.with_field(field(name)));
            if let Some(value) = value {
                diagnostic = diagnostic.with_value(value);
            }
            diagnostic
        };

        let thumbnail_loc = match raw.thumbnail_loc.as_deref() {
            None => {
                self.emit(out, missing(location, &field("thumbnailLoc")));
                None
            }
            Some(value) => self.parse_url(value, location, &field("thumbnailLoc"), out),
        };

        let title = self.required_text(raw.title.as_deref(), location, &field("title"), out);
        if let Some(title) = &title {
            let length = title.chars().count();
            if length > MAX_VIDEO_TITLE_CHARS {
                self.emit(
                    out,
                    error(
                        DiagnosticCode::VideoTitleTooLong,
                        format!(
                            "video title has {length} characters; Google's schema allows {MAX_VIDEO_TITLE_CHARS}"
                        ),
                        "title",
                        Some(title),
                    )
                    .with_suggestion("Shorten the title to 100 characters."),
                );
            }
        }
        let description = self.required_text(
            raw.description.as_deref(),
            location,
            &field("description"),
            out,
        );
        if let Some(description) = &description {
            let length = description.chars().count();
            if length > MAX_VIDEO_DESCRIPTION_CHARS {
                self.emit(
                    out,
                    error(
                        DiagnosticCode::VideoTextInvalid,
                        format!(
                            "video description has {length} characters; at most {MAX_VIDEO_DESCRIPTION_CHARS} are allowed"
                        ),
                        "description",
                        None,
                    ),
                );
            }
        }

        let content_loc = raw
            .content_loc
            .as_deref()
            .and_then(|v| self.parse_url(v, location, &field("contentLoc"), out));
        let player_loc = raw
            .player_loc
            .as_deref()
            .and_then(|v| self.parse_url(v, location, &field("playerLoc"), out));
        if raw.content_loc.is_none() && raw.player_loc.is_none() {
            self.emit(
                out,
                error(
                    DiagnosticCode::VideoLocationMissing,
                    "a video needs contentLoc or playerLoc".to_owned(),
                    "contentLoc",
                    None,
                )
                .with_suggestion(
                    "Give the URL of the media file (contentLoc) or of its player (playerLoc).",
                ),
            );
        }
        if let Some(page) = page {
            for (name, url) in [("contentLoc", &content_loc), ("playerLoc", &player_loc)] {
                if url.as_ref().is_some_and(|u| u.as_str() == page.as_str()) {
                    self.emit(
                        out,
                        error(
                            DiagnosticCode::VideoLocationSameAsPage,
                            format!("video {name} must differ from the page loc"),
                            name,
                            Some(page.as_str()),
                        ),
                    );
                }
            }
        }

        let duration = match raw.duration {
            None => None,
            Some(value) => match u32::try_from(value) {
                Ok(seconds) if (1..=MAX_VIDEO_DURATION_SECONDS).contains(&seconds) => Some(seconds),
                _ => {
                    self.emit(
                        out,
                        error(
                            DiagnosticCode::VideoDurationOutOfRange,
                            format!(
                                "video duration must be between 1 and {MAX_VIDEO_DURATION_SECONDS} seconds"
                            ),
                            "duration",
                            Some(&value.to_string()),
                        ),
                    );
                    None
                }
            },
        };

        let rating = match raw.rating {
            None => None,
            Some(value) if (0.0..=5.0).contains(&value) => Some(value),
            Some(value) => {
                self.emit(
                    out,
                    error(
                        DiagnosticCode::VideoRatingOutOfRange,
                        "video rating must be between 0.0 and 5.0".to_owned(),
                        "rating",
                        Some(&value.to_string()),
                    ),
                );
                None
            }
        };

        let expiration_date = self.video_date(
            raw.expiration_date.as_deref(),
            location,
            &field("expirationDate"),
            out,
        );
        if let (Some(expiration), Some(reference)) =
            (&expiration_date, &self.options.reference_time)
        {
            if expiration.cmp_instant(reference) != Ordering::Greater {
                self.emit(
                    out,
                    error(
                        DiagnosticCode::VideoExpired,
                        "video expiration date is not later than the reference time; Google will not show it".to_owned(),
                        "expirationDate",
                        Some(expiration.as_str()),
                    )
                    .with_suggestion("Remove the video or update its expiration date."),
                );
            }
        }
        let publication_date = self.video_date(
            raw.publication_date.as_deref(),
            location,
            &field("publicationDate"),
            out,
        );

        if raw.tags.len() > MAX_VIDEO_TAGS {
            self.emit(
                out,
                error(
                    DiagnosticCode::VideoTagLimitExceeded,
                    format!(
                        "{} tags listed; at most {MAX_VIDEO_TAGS} are allowed per video",
                        raw.tags.len()
                    ),
                    "tags",
                    None,
                ),
            );
        }
        let mut tags = Vec::with_capacity(raw.tags.len());
        for (i, tag) in raw.tags.iter().enumerate() {
            if let Some(tag) =
                self.required_text(Some(tag), location, &field(&format!("tags[{i}]")), out)
            {
                tags.push(tag);
            }
        }

        let restriction = raw.restriction.as_ref().and_then(|r| {
            let relationship = self.relationship(
                r.relationship.as_deref(),
                DiagnosticCode::VideoRestrictionInvalid,
                location,
                &field("restriction.relationship"),
                out,
            );
            let mut countries: Vec<CountryCode> = Vec::with_capacity(r.countries.len());
            let mut ok = true;
            for (i, code) in r.countries.iter().enumerate() {
                match CountryCode::parse(code) {
                    Some(country) if !countries.contains(&country) => countries.push(country),
                    Some(_) => {}
                    None => {
                        ok = false;
                        self.emit(
                            out,
                            error(
                                DiagnosticCode::VideoRestrictionInvalid,
                                "country is not an ISO 3166-1 alpha-2 code (two letters)"
                                    .to_owned(),
                                &format!("restriction.countries[{i}]"),
                                Some(code),
                            ),
                        );
                    }
                }
            }
            ok.then_some(VideoRestriction {
                relationship: relationship?,
                countries,
            })
        });

        let platform = raw.platform.as_ref().and_then(|p| {
            let relationship = self.relationship(
                p.relationship.as_deref(),
                DiagnosticCode::VideoPlatformInvalid,
                location,
                &field("platform.relationship"),
                out,
            );
            let mut platforms: Vec<Platform> = Vec::with_capacity(p.platforms.len());
            let mut ok = true;
            for (i, value) in p.platforms.iter().enumerate() {
                match Platform::parse(value) {
                    Some(platform) if !platforms.contains(&platform) => platforms.push(platform),
                    Some(_) => {}
                    None => {
                        ok = false;
                        self.emit(
                            out,
                            error(
                                DiagnosticCode::VideoPlatformInvalid,
                                "platform must be web, mobile or tv".to_owned(),
                                &format!("platform.platforms[{i}]"),
                                Some(value),
                            ),
                        );
                    }
                }
            }
            ok.then_some(VideoPlatform {
                relationship: relationship?,
                platforms,
            })
        });

        let uploader = raw.uploader.as_ref().and_then(|u| {
            let name = u.name.as_deref().map(str::trim).unwrap_or_default();
            let length = name.chars().count();
            if name.is_empty() || length > MAX_VIDEO_UPLOADER_CHARS {
                self.emit(
                    out,
                    error(
                        DiagnosticCode::VideoUploaderInvalid,
                        format!(
                            "video uploader must have 1 to {MAX_VIDEO_UPLOADER_CHARS} characters, found {length}"
                        ),
                        "uploader.name",
                        None,
                    ),
                );
                return None;
            }
            let info = u
                .info
                .as_deref()
                .and_then(|v| self.parse_url(v, location, &field("uploader.info"), out));
            if let (Some(info), Some(page)) = (&info, page) {
                if info.host() != page.host() {
                    self.emit(
                        out,
                        error(
                            DiagnosticCode::VideoUploaderHostMismatch,
                            "uploader info page should be on the same host as the page loc".to_owned(),
                            "uploader.info",
                            Some(info.as_str()),
                        ),
                    );
                }
            }
            Some(VideoUploader {
                name: name.to_owned(),
                info,
            })
        });

        let has_error = out[first..].iter().any(Diagnostic::is_error);
        if has_error {
            return None;
        }
        Some(Video {
            thumbnail_loc: thumbnail_loc?,
            title: title?,
            description: description?,
            content_loc,
            player_loc,
            duration,
            expiration_date,
            rating,
            view_count: raw.view_count,
            publication_date,
            tags,
            family_friendly: raw.family_friendly,
            restriction,
            requires_subscription: raw.requires_subscription,
            uploader,
            platform,
            live: raw.live,
        })
    }

    /// A required, non-empty text. Surrounding whitespace is kept out.
    fn required_text(
        &self,
        value: Option<&str>,
        location: &Location,
        field: &str,
        out: &mut Vec<Diagnostic>,
    ) -> Option<String> {
        match value.map(str::trim) {
            None => {
                self.emit(out, missing(location, field));
                None
            }
            Some("") => {
                self.emit(
                    out,
                    Diagnostic::new(
                        DiagnosticCode::VideoTextInvalid,
                        format!("`{field}` is empty"),
                        location.with_field(field),
                    ),
                );
                None
            }
            Some(text) => Some(text.to_owned()),
        }
    }

    fn video_date(
        &self,
        value: Option<&str>,
        location: &Location,
        field: &str,
        out: &mut Vec<Diagnostic>,
    ) -> Option<W3cDateTime> {
        let value = value?;
        match W3cDateTime::parse(value) {
            Ok(date) => Some(date),
            Err(error) => {
                self.emit(
                    out,
                    Diagnostic::new(
                        DiagnosticCode::VideoDateInvalid,
                        format!("not a W3C Datetime: {error}"),
                        location.with_field(field),
                    )
                    .with_value(value),
                );
                None
            }
        }
    }

    fn relationship(
        &self,
        value: Option<&str>,
        code: DiagnosticCode,
        location: &Location,
        field: &str,
        out: &mut Vec<Diagnostic>,
    ) -> Option<Relationship> {
        let parsed = value.and_then(Relationship::parse);
        if parsed.is_none() {
            let mut diagnostic = Diagnostic::new(
                code,
                "relationship must be \"allow\" or \"deny\"",
                location.with_field(field),
            );
            if let Some(value) = value {
                diagnostic = diagnostic.with_value(value);
            }
            self.emit(out, diagnostic);
        }
        parsed
    }
}

#[cfg(test)]
mod tests {
    use sitemap_model::{RawVideoPlatform, RawVideoRestriction, RawVideoUploader};

    use super::*;
    use crate::ValidationOptions;

    fn base_video() -> RawVideo {
        RawVideo {
            thumbnail_loc: Some("https://example.com/t.jpg".into()),
            title: Some("Grilling steaks".into()),
            description: Some("How to grill steaks.".into()),
            content_loc: Some("https://cdn.example.com/v.mp4".into()),
            ..RawVideo::default()
        }
    }

    fn check(video: RawVideo, options: ValidationOptions) -> (Option<Video>, Vec<&'static str>) {
        let raw = RawUrlRecord {
            loc: Some("https://example.com/watch".into()),
            videos: vec![video],
            ..RawUrlRecord::default()
        };
        let mut out = Vec::new();
        let entry = Validator::new(options).validate(&raw, &Location::record(1, None), &mut out);
        let codes = out.iter().map(|d| d.code.as_str()).collect();
        (entry.and_then(|e| e.videos.into_iter().next()), codes)
    }

    #[test]
    fn minimal_video_is_valid() {
        let (video, codes) = check(base_video(), ValidationOptions::default());
        assert!(codes.is_empty(), "{codes:?}");
        assert_eq!(video.unwrap().title, "Grilling steaks");
    }

    #[test]
    fn full_video_is_normalized() {
        let video = RawVideo {
            player_loc: Some("https://example.com/player?v=1".into()),
            duration: Some(600),
            rating: Some(4.2),
            view_count: Some(12),
            expiration_date: Some("2030-01-01".into()),
            publication_date: Some("2026-01-01T10:00:00+01:00".into()),
            tags: vec!["steak".into(), "grill".into()],
            family_friendly: Some(true),
            restriction: Some(RawVideoRestriction {
                relationship: Some("allow".into()),
                countries: vec!["fr".into(), "BE".into(), "FR".into()],
            }),
            platform: Some(RawVideoPlatform {
                relationship: Some("deny".into()),
                platforms: vec!["tv".into()],
            }),
            requires_subscription: Some(false),
            uploader: Some(RawVideoUploader {
                name: Some("Chef".into()),
                info: Some("https://example.com/chef".into()),
            }),
            live: Some(false),
            ..base_video()
        };
        let (video, codes) = check(video, ValidationOptions::default());
        assert!(codes.is_empty(), "{codes:?}");
        let video = video.unwrap();
        let countries: Vec<&str> = video
            .restriction
            .as_ref()
            .unwrap()
            .countries
            .iter()
            .map(CountryCode::as_str)
            .collect();
        assert_eq!(countries, ["FR", "BE"]);
    }

    #[test]
    fn every_rule_reports_its_code() {
        let video = RawVideo {
            thumbnail_loc: None,
            title: Some("  ".into()),
            description: Some("d".repeat(2049)),
            content_loc: Some("https://example.com/watch".into()),
            duration: Some(0),
            rating: Some(5.5),
            expiration_date: Some("tomorrow".into()),
            tags: vec!["t".into(); 33],
            restriction: Some(RawVideoRestriction {
                relationship: Some("only".into()),
                countries: vec!["FRA".into()],
            }),
            platform: Some(RawVideoPlatform {
                relationship: Some("allow".into()),
                platforms: vec!["desktop".into()],
            }),
            uploader: Some(RawVideoUploader {
                name: Some(String::new()),
                info: None,
            }),
            ..RawVideo::default()
        };
        let (video, codes) = check(video, ValidationOptions::default());
        assert!(video.is_none());
        for code in [
            "REQUIRED_FIELD_MISSING",
            "VIDEO_TEXT_INVALID",
            "VIDEO_LOCATION_SAME_AS_PAGE",
            "VIDEO_DURATION_OUT_OF_RANGE",
            "VIDEO_RATING_OUT_OF_RANGE",
            "VIDEO_DATE_INVALID",
            "VIDEO_TAG_LIMIT_EXCEEDED",
            "VIDEO_RESTRICTION_INVALID",
            "VIDEO_PLATFORM_INVALID",
            "VIDEO_UPLOADER_INVALID",
        ] {
            assert!(codes.contains(&code), "{code} missing from {codes:?}");
        }
    }

    #[test]
    fn location_is_required_and_warnings_keep_the_video() {
        let (_, codes) = check(
            RawVideo {
                content_loc: None,
                ..base_video()
            },
            ValidationOptions::default(),
        );
        assert_eq!(codes, ["VIDEO_LOCATION_MISSING"]);

        let video = RawVideo {
            title: Some("t".repeat(101)),
            expiration_date: Some("2026-01-01".into()),
            uploader: Some(RawVideoUploader {
                name: Some("Chef".into()),
                info: Some("https://other.example/chef".into()),
            }),
            ..base_video()
        };
        let options = ValidationOptions {
            reference_time: Some(W3cDateTime::parse("2026-11-21T00:00:00Z").unwrap()),
            ..ValidationOptions::default()
        };
        let (video, codes) = check(video, options);
        assert_eq!(
            codes,
            [
                "VIDEO_TITLE_TOO_LONG",
                "VIDEO_EXPIRED",
                "VIDEO_UPLOADER_HOST_MISMATCH"
            ]
        );
        assert!(video.is_some(), "warnings do not drop the video");
    }
}
