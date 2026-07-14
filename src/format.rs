//! Display formatting helpers.

use crate::{Attachment, Location};

/// Format a location for display.
///
/// Returns a string like "Paris, France" or "Eiffel Tower, France, (48.8566, 2.3522)".
pub fn format_location(location: &Location) -> String {
    let mut parts = Vec::new();
    if let Some(ref place) = location.place {
        parts.push(place.clone());
    }
    parts.push(location.country.clone());
    if let Some(ref coords) = location.coordinates {
        parts.push(format!("({:.4}, {:.4})", coords.lat, coords.lon));
    }
    parts.join(", ")
}

/// Format a date for display.
///
/// Currently just returns the date string as-is, but could be extended
/// to provide more human-readable formatting.
pub fn format_date_display(date_str: &str) -> String {
    date_str.to_string()
}

/// Format an attachment for display.
///
/// Returns the title with URL, or just URL if no title.
pub fn format_attachment(attachment: &Attachment) -> String {
    if let Some(ref title) = attachment.title {
        format!("{} ({})", title, attachment.url)
    } else {
        attachment.url.to_string()
    }
}

/// Format an attachment for Markdown display.
///
/// Returns a markdown link.
pub fn format_attachment_markdown(attachment: &Attachment) -> String {
    if let Some(ref title) = attachment.title {
        format!("[{}]({})", title, attachment.url)
    } else {
        format!("[link]({})", attachment.url)
    }
}

/// Truncate text to a maximum length in characters, adding ellipsis if needed.
///
/// Counts characters (not bytes), so multi-byte UTF-8 content is safe.
/// The result never exceeds `max_len` characters.
pub fn truncate_text(s: &str, max_len: usize) -> String {
    if s.chars().count() <= max_len {
        s.to_string()
    } else if max_len <= 3 {
        ".".repeat(max_len)
    } else {
        let truncated: String = s.chars().take(max_len - 3).collect();
        format!("{truncated}...")
    }
}

/// Escape a string for CSV output.
///
/// Wraps in quotes if contains comma, quote, or newline.
pub fn escape_csv(s: &str) -> String {
    if s.contains(',') || s.contains('"') || s.contains('\n') {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

/// Escape a string for use inside a Markdown table cell.
///
/// Pipes are escaped and newlines become `<br>` so multi-line text
/// cannot break the table structure.
pub fn escape_md(s: &str) -> String {
    s.replace('|', "\\|")
        .replace('\r', "")
        .replace('\n', "<br>")
}

/// Parse a `#RRGGBB` hex color into an `(r, g, b)` triple.
pub fn parse_hex_color(hex: &str) -> Option<(u8, u8, u8)> {
    let hex = hex.strip_prefix('#')?;
    if hex.len() != 6 || !hex.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
    let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
    let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
    Some((r, g, b))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Coordinates;

    #[test]
    fn test_format_location_country_only() {
        let loc = Location::new("France");
        assert_eq!(format_location(&loc), "France");
    }

    #[test]
    fn test_format_location_with_place() {
        let loc = Location::new("France").with_place("Paris");
        assert_eq!(format_location(&loc), "Paris, France");
    }

    #[test]
    fn test_format_location_with_coords() {
        let loc = Location::new("France")
            .with_place("Eiffel Tower, Paris")
            .with_coordinates(Coordinates::new(48.8566, 2.3522));
        assert_eq!(
            format_location(&loc),
            "Eiffel Tower, Paris, France, (48.8566, 2.3522)"
        );
    }

    #[test]
    fn test_format_attachment_with_title() {
        let att = Attachment::new("https://example.com/photo.jpg").with_title("My Photo");
        assert_eq!(
            format_attachment(&att),
            "My Photo (https://example.com/photo.jpg)"
        );
    }

    #[test]
    fn test_format_attachment_without_title() {
        let att = Attachment::new("https://example.com/photo.jpg");
        assert_eq!(format_attachment(&att), "https://example.com/photo.jpg");
    }

    #[test]
    fn test_format_attachment_markdown() {
        let att = Attachment::new("https://example.com/photo.jpg").with_title("My Photo");
        assert_eq!(
            format_attachment_markdown(&att),
            "[My Photo](https://example.com/photo.jpg)"
        );
    }

    #[test]
    fn test_truncate_text_short() {
        assert_eq!(truncate_text("Hello", 10), "Hello");
    }

    #[test]
    fn test_truncate_text_long() {
        assert_eq!(truncate_text("Hello World!", 8), "Hello...");
    }

    #[test]
    fn test_truncate_text_multibyte_at_boundary() {
        // Byte-index slicing used to panic here ('ä' spans two bytes at the cut)
        let s = "Wanderung auf die Mettmenalp, Schwändi GL";
        let t = truncate_text(s, 38);
        assert!(t.ends_with("..."));
        assert_eq!(t.chars().count(), 38);
    }

    #[test]
    fn test_truncate_text_cjk() {
        assert_eq!(truncate_text("日本語のテキストです", 8), "日本語のテ...");
    }

    #[test]
    fn test_truncate_text_tiny_max_len() {
        assert_eq!(truncate_text("abcdef", 3), "...");
        assert_eq!(truncate_text("abcdef", 2), "..");
        assert_eq!(truncate_text("abcdef", 0), "");
        assert_eq!(truncate_text("ab", 2), "ab");
    }

    #[test]
    fn test_escape_csv_simple() {
        assert_eq!(escape_csv("hello"), "hello");
    }

    #[test]
    fn test_escape_csv_with_comma() {
        assert_eq!(escape_csv("hello, world"), "\"hello, world\"");
    }

    #[test]
    fn test_escape_csv_with_quote() {
        assert_eq!(escape_csv("say \"hello\""), "\"say \"\"hello\"\"\"");
    }

    #[test]
    fn test_escape_md() {
        assert_eq!(escape_md("plain text"), "plain text");
        assert_eq!(escape_md("a | b"), "a \\| b");
        assert_eq!(escape_md("line1\nline2"), "line1<br>line2");
        assert_eq!(escape_md("line1\r\nline2"), "line1<br>line2");
    }

    #[test]
    fn test_parse_hex_color() {
        assert_eq!(parse_hex_color("#4A90D9"), Some((0x4A, 0x90, 0xD9)));
        assert_eq!(parse_hex_color("#ffffff"), Some((255, 255, 255)));
        assert_eq!(parse_hex_color("#000000"), Some((0, 0, 0)));
        assert_eq!(parse_hex_color("4A90D9"), None, "missing #");
        assert_eq!(parse_hex_color("#4A90D"), None, "too short");
        assert_eq!(parse_hex_color("#GGGGGG"), None, "not hex");
    }
}
