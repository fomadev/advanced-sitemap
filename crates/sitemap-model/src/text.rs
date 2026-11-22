//! Characters that XML 1.0 text cannot contain.
//!
//! Shared by validation (which reports them as diagnostics) and the XML
//! writer (which refuses them), so that a validated entry always encodes.

/// The first character of `text` that XML 1.0 forbids (C0 controls other
/// than tab, line feed and carriage return, and U+FFFE, U+FFFF), if any.
pub fn xml_forbidden_char(text: &str) -> Option<char> {
    text.chars().find(|&ch| is_xml_forbidden(ch))
}

/// `true` when XML 1.0 forbids `ch` in text.
pub fn is_xml_forbidden(ch: char) -> bool {
    matches!(ch, '\u{0}'..='\u{8}' | '\u{B}' | '\u{C}' | '\u{E}'..='\u{1F}' | '\u{FFFE}' | '\u{FFFF}')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn forbidden_characters() {
        assert_eq!(xml_forbidden_char("ok\ttab\nline\rcr é 日本"), None);
        assert_eq!(xml_forbidden_char("a\u{0}b"), Some('\u{0}'));
        assert_eq!(xml_forbidden_char("\u{1F}"), Some('\u{1F}'));
        assert_eq!(xml_forbidden_char("\u{FFFE}"), Some('\u{FFFE}'));
        assert!(!is_xml_forbidden('\u{7F}'));
    }
}
