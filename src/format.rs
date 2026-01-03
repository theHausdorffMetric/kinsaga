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

/// Truncate text to a maximum length, adding ellipsis if needed.
pub fn truncate_text(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        format!("{}...", &s[..max_len.saturating_sub(3)])
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Coordinates;
    use url::Url;

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
        assert_eq!(format_location(&loc), "Eiffel Tower, Paris, France, (48.8566, 2.3522)");
    }

    #[test]
    fn test_format_attachment_with_title() {
        let url = Url::parse("https://example.com/photo.jpg").unwrap();
        let att = Attachment::new(url).with_title("My Photo");
        assert_eq!(
            format_attachment(&att),
            "My Photo (https://example.com/photo.jpg)"
        );
    }

    #[test]
    fn test_format_attachment_without_title() {
        let url = Url::parse("https://example.com/photo.jpg").unwrap();
        let att = Attachment::new(url);
        assert_eq!(format_attachment(&att), "https://example.com/photo.jpg");
    }

    #[test]
    fn test_format_attachment_markdown() {
        let url = Url::parse("https://example.com/photo.jpg").unwrap();
        let att = Attachment::new(url).with_title("My Photo");
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
}
