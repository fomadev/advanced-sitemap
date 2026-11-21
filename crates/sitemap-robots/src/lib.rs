//! Validated, deterministic `robots.txt` generation (RFC 9309).
//!
//! [`RobotsConfig::build`] checks groups, rules and `Sitemap` URLs, reports
//! every problem as a [`Diagnostic`], and returns a [`RobotsTxt`] when there
//! is no error. Rendering is deterministic: no timestamp, LF line endings,
//! groups in input order, `Disallow` then `Allow` lines in input order, then
//! the `Sitemap` records.
//!
//! The engine never decides what to block: rules come from the caller.

use std::collections::HashSet;
use std::fmt::Write as _;

use sitemap_model::{Diagnostic, DiagnosticCode, Location, SitemapUrl};

/// Crawlers must parse at least 500 KiB of robots.txt (RFC 9309, 2.5).
pub const MAX_ROBOTS_BYTES: usize = 500 * 1024;

/// One group: user agents and their rules.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Deserialize, serde::Serialize))]
#[cfg_attr(
    feature = "serde",
    serde(deny_unknown_fields, rename_all = "camelCase")
)]
pub struct RobotsGroup {
    /// Product tokens, or `*` for every crawler.
    pub user_agents: Vec<String>,
    /// Path patterns that may not be crawled. `""` disallows nothing.
    #[cfg_attr(
        feature = "serde",
        serde(default, skip_serializing_if = "Vec::is_empty")
    )]
    pub disallow: Vec<String>,
    /// Path patterns that may be crawled (longest match wins, RFC 9309).
    #[cfg_attr(
        feature = "serde",
        serde(default, skip_serializing_if = "Vec::is_empty")
    )]
    pub allow: Vec<String>,
}

/// Unchecked robots.txt configuration.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Deserialize, serde::Serialize))]
#[cfg_attr(
    feature = "serde",
    serde(deny_unknown_fields, rename_all = "camelCase")
)]
pub struct RobotsConfig {
    /// Groups, in output order.
    #[cfg_attr(feature = "serde", serde(default))]
    pub groups: Vec<RobotsGroup>,
    /// Extra `Sitemap` URLs (absolute). The engine adds the sitemap it
    /// generates when it knows its public URL.
    #[cfg_attr(
        feature = "serde",
        serde(default, skip_serializing_if = "Vec::is_empty")
    )]
    pub sitemaps: Vec<String>,
}

/// A validated robots.txt.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RobotsTxt {
    groups: Vec<RobotsGroup>,
    sitemaps: Vec<SitemapUrl>,
}

impl RobotsConfig {
    /// Validates the configuration. Returns the robots.txt when no
    /// error-level diagnostic was produced, and every diagnostic.
    pub fn build(&self) -> (Option<RobotsTxt>, Vec<Diagnostic>) {
        let mut out = Vec::new();
        let mut groups = Vec::with_capacity(self.groups.len());
        let mut seen_agents: HashSet<String> = HashSet::new();

        for (g, group) in self.groups.iter().enumerate() {
            let at = |field: String| Location::default().with_field(field);
            if group.user_agents.is_empty() {
                out.push(
                    Diagnostic::new(
                        DiagnosticCode::RobotsGroupInvalid,
                        "a group needs at least one user agent",
                        at(format!("robots.groups[{g}].userAgents")),
                    )
                    .with_suggestion("Use \"*\" to address every crawler."),
                );
            }
            let mut agents = Vec::with_capacity(group.user_agents.len());
            for (a, agent) in group.user_agents.iter().enumerate() {
                let field = format!("robots.groups[{g}].userAgents[{a}]");
                let agent = agent.trim();
                if agent.is_empty()
                    || agent
                        .chars()
                        .any(|c| c.is_whitespace() || c.is_control() || c == ':' || c == '#')
                {
                    out.push(
                        Diagnostic::new(
                            DiagnosticCode::RobotsUserAgentInvalid,
                            "user agent must be a product token without spaces, ':' or '#'",
                            at(field),
                        )
                        .with_value(agent),
                    );
                    continue;
                }
                let standard = agent == "*"
                    || agent
                        .bytes()
                        .all(|b| b.is_ascii_alphabetic() || b == b'-' || b == b'_');
                if !standard {
                    out.push(
                        Diagnostic::new(
                            DiagnosticCode::RobotsUserAgentNonstandard,
                            "RFC 9309 product tokens contain only letters, '-' and '_'; some crawlers may not match it",
                            at(field.clone()),
                        )
                        .with_value(agent),
                    );
                }
                if !seen_agents.insert(agent.to_ascii_lowercase()) {
                    out.push(
                        Diagnostic::new(
                            DiagnosticCode::RobotsDuplicateUserAgent,
                            "this user agent already has a group; crawlers merge the groups",
                            at(field),
                        )
                        .with_value(agent)
                        .with_suggestion("Put all rules for one crawler in one group."),
                    );
                }
                agents.push(agent.to_owned());
            }
            let disallow = check_paths(&group.disallow, g, "disallow", &mut out);
            let allow = check_paths(&group.allow, g, "allow", &mut out);
            groups.push(RobotsGroup {
                user_agents: agents,
                disallow,
                allow,
            });
        }

        let mut sitemaps = Vec::with_capacity(self.sitemaps.len());
        for (i, url) in self.sitemaps.iter().enumerate() {
            match SitemapUrl::new(url) {
                Ok(url) => {
                    if !sitemaps.contains(&url) {
                        sitemaps.push(url);
                    }
                }
                Err(error) => out.push(
                    Diagnostic::new(
                        DiagnosticCode::UrlInvalid,
                        format!("Sitemap URL is invalid: {error}"),
                        Location::default().with_field(format!("robots.sitemaps[{i}]")),
                    )
                    .with_value(url)
                    .with_suggestion("Sitemap records need an absolute http(s) URL."),
                ),
            }
        }

        if out.iter().any(Diagnostic::is_error) {
            return (None, out);
        }
        (Some(RobotsTxt { groups, sitemaps }), out)
    }
}

/// Checks and normalizes path patterns: non-ASCII characters are
/// percent-encoded as UTF-8 so that the output is ASCII.
fn check_paths(
    paths: &[String],
    group: usize,
    kind: &str,
    out: &mut Vec<Diagnostic>,
) -> Vec<String> {
    let mut result = Vec::with_capacity(paths.len());
    for (i, path) in paths.iter().enumerate() {
        let valid_start = path.is_empty() || path.starts_with('/') || path.starts_with('*');
        let bad_char = path
            .chars()
            .any(|c| c.is_whitespace() || c.is_control() || c == '#');
        if !valid_start || bad_char {
            out.push(
                Diagnostic::new(
                    DiagnosticCode::RobotsPathInvalid,
                    "path must be empty or start with '/' or '*', without spaces, control characters or '#'",
                    Location::default().with_field(format!("robots.groups[{group}].{kind}[{i}]")),
                )
                .with_value(path)
                .with_suggestion("Use a path relative to the site root, for example \"/private/\"."),
            );
            continue;
        }
        let mut encoded = String::with_capacity(path.len());
        for c in path.chars() {
            if c.is_ascii() {
                encoded.push(c);
            } else {
                let mut buf = [0u8; 4];
                for byte in c.encode_utf8(&mut buf).bytes() {
                    let _ = write!(encoded, "%{byte:02X}");
                }
            }
        }
        result.push(encoded);
    }
    result
}

fn rule(text: &mut String, name: &str, path: &str) {
    if path.is_empty() {
        let _ = writeln!(text, "{name}:");
    } else {
        let _ = writeln!(text, "{name}: {path}");
    }
}

impl RobotsTxt {
    /// Adds a `Sitemap` record (ignored when already present).
    pub fn add_sitemap(&mut self, url: SitemapUrl) {
        if !self.sitemaps.contains(&url) {
            self.sitemaps.push(url);
        }
    }

    /// `Sitemap` records, in output order.
    pub fn sitemaps(&self) -> &[SitemapUrl] {
        &self.sitemaps
    }

    /// The robots.txt text.
    pub fn render(&self) -> String {
        let mut text = String::new();
        for (i, group) in self.groups.iter().enumerate() {
            if i > 0 {
                text.push('\n');
            }
            for agent in &group.user_agents {
                let _ = writeln!(text, "User-agent: {agent}");
            }
            for path in &group.disallow {
                rule(&mut text, "Disallow", path);
            }
            for path in &group.allow {
                rule(&mut text, "Allow", path);
            }
        }
        if !self.sitemaps.is_empty() {
            if !self.groups.is_empty() {
                text.push('\n');
            }
            for url in &self.sitemaps {
                let _ = writeln!(text, "Sitemap: {}", url.as_str());
            }
        }
        text
    }

    /// Renders and checks the size against [`MAX_ROBOTS_BYTES`].
    pub fn render_checked(&self) -> (String, Option<Diagnostic>) {
        let text = self.render();
        let warning = (text.len() > MAX_ROBOTS_BYTES).then(|| {
            Diagnostic::new(
                DiagnosticCode::RobotsTooLarge,
                format!(
                    "robots.txt is {} bytes; crawlers only have to read {MAX_ROBOTS_BYTES}",
                    text.len()
                ),
                Location::artifact("robots.txt"),
            )
        });
        (text, warning)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn group(agents: &[&str], disallow: &[&str], allow: &[&str]) -> RobotsGroup {
        let owned = |v: &[&str]| v.iter().map(|s| (*s).to_owned()).collect();
        RobotsGroup {
            user_agents: owned(agents),
            disallow: owned(disallow),
            allow: owned(allow),
        }
    }

    fn codes(diagnostics: &[Diagnostic]) -> Vec<&'static str> {
        diagnostics.iter().map(|d| d.code.as_str()).collect()
    }

    #[test]
    fn renders_groups_rules_and_sitemaps_deterministically() {
        let config = RobotsConfig {
            groups: vec![
                group(
                    &["Googlebot", "Bingbot"],
                    &["/private/"],
                    &["/private/public.html"],
                ),
                group(&["*"], &[""], &[]),
            ],
            sitemaps: vec!["https://example.com/extra.xml".into()],
        };
        let (robots, diagnostics) = config.build();
        assert!(diagnostics.is_empty(), "{diagnostics:?}");
        let mut robots = robots.unwrap();
        robots.add_sitemap(SitemapUrl::new("https://example.com/sitemap.xml").unwrap());
        robots.add_sitemap(SitemapUrl::new("https://example.com/extra.xml").unwrap());
        assert_eq!(
            robots.render(),
            "User-agent: Googlebot\nUser-agent: Bingbot\nDisallow: /private/\nAllow: /private/public.html\n\nUser-agent: *\nDisallow:\n\nSitemap: https://example.com/extra.xml\nSitemap: https://example.com/sitemap.xml\n"
        );
    }

    #[test]
    fn sitemaps_only() {
        let config = RobotsConfig {
            groups: vec![],
            sitemaps: vec!["https://example.com/sitemap.xml".into()],
        };
        let (robots, _) = config.build();
        assert_eq!(
            robots.unwrap().render(),
            "Sitemap: https://example.com/sitemap.xml\n"
        );
    }

    #[test]
    fn non_ascii_paths_are_percent_encoded() {
        let config = RobotsConfig {
            groups: vec![group(&["*"], &["/café/*.pdf$"], &[])],
            sitemaps: vec![],
        };
        let (robots, _) = config.build();
        assert!(
            robots
                .unwrap()
                .render()
                .contains("Disallow: /caf%C3%A9/*.pdf$\n")
        );
    }

    #[test]
    fn errors_and_warnings() {
        let config = RobotsConfig {
            groups: vec![
                group(&[], &["/a"], &[]),
                group(
                    &["Google bot", "MJ12bot", "googlebot"],
                    &["private", "/x#y"],
                    &[],
                ),
                group(&["Googlebot"], &[], &[]),
            ],
            sitemaps: vec!["/sitemap.xml".into()],
        };
        let (robots, diagnostics) = config.build();
        assert!(robots.is_none());
        assert_eq!(
            codes(&diagnostics),
            [
                "ROBOTS_GROUP_INVALID",
                "ROBOTS_USER_AGENT_INVALID",
                "ROBOTS_USER_AGENT_NONSTANDARD",
                "ROBOTS_PATH_INVALID",
                "ROBOTS_PATH_INVALID",
                "ROBOTS_DUPLICATE_USER_AGENT",
                "URL_INVALID",
            ]
        );
    }

    #[test]
    fn size_warning() {
        let paths: Vec<String> = (0..60_000).map(|i| format!("/p/{i}")).collect();
        let config = RobotsConfig {
            groups: vec![RobotsGroup {
                user_agents: vec!["*".into()],
                disallow: paths,
                allow: vec![],
            }],
            sitemaps: vec![],
        };
        let (robots, _) = config.build();
        let (text, warning) = robots.unwrap().render_checked();
        assert!(text.len() > MAX_ROBOTS_BYTES);
        assert_eq!(warning.unwrap().code, DiagnosticCode::RobotsTooLarge);
    }
}
