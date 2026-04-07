//! URL detection in terminal content
//!
//! Scans terminal grid for URLs and provides highlighting information.

use regex::Regex;
use once_cell::sync::Lazy;

/// A detected URL in the terminal
#[derive(Debug, Clone)]
pub struct DetectedUrl {
    /// Start column
    pub start_col: u16,
    /// End column (exclusive)
    pub end_col: u16,
    /// Row
    pub row: u16,
    /// The URL text
    pub url: String,
}

/// URL pattern for detection
static URL_REGEX: Lazy<Regex> = Lazy::new(|| {
    // Match common URL schemes
    Regex::new(
        r"(?i)(https?://|ftp://|file://|mailto:|ssh://|git://)[^\s<>\[\]{}|\\^`'\x00-\x1f\x7f]+"
    ).expect("Invalid URL regex")
});

/// Detect URLs in a line of text
pub fn detect_urls_in_line(text: &str, row: u16) -> Vec<DetectedUrl> {
    let mut urls = Vec::new();
    
    for mat in URL_REGEX.find_iter(text) {
        let url_text = mat.as_str();
        
        // Clean up trailing punctuation that's likely not part of the URL
        let trimmed = url_text.trim_end_matches(|c| matches!(c, '.' | ',' | ':' | ';' | '!' | '?' | ')' | ']' | '\'' | '"'));
        
        urls.push(DetectedUrl {
            start_col: mat.start() as u16,
            end_col: (mat.start() + trimmed.len()) as u16,
            row,
            url: trimmed.to_string(),
        });
    }
    
    urls
}

/// Detect URLs in a grid
pub fn detect_urls_in_grid<F>(rows: u16, cols: u16, get_line: F) -> Vec<DetectedUrl>
where
    F: Fn(u16) -> String,
{
    let mut urls = Vec::new();
    
    for row in 0..rows {
        let line = get_line(row);
        urls.extend(detect_urls_in_line(&line, row));
    }
    
    urls
}

/// Check if a position is within a URL
pub fn url_at_position(urls: &[DetectedUrl], col: u16, row: u16) -> Option<&DetectedUrl> {
    urls.iter().find(|url| url.row == row && col >= url.start_col && col < url.end_col)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_http_url() {
        let urls = detect_urls_in_line("Check out https://example.com for more", 0);
        assert_eq!(urls.len(), 1);
        assert_eq!(urls[0].url, "https://example.com");
        assert_eq!(urls[0].start_col, 10);
    }

    #[test]
    fn detect_multiple_urls() {
        let urls = detect_urls_in_line("Visit https://a.com and http://b.com", 0);
        assert_eq!(urls.len(), 2);
        assert_eq!(urls[0].url, "https://a.com");
        assert_eq!(urls[1].url, "http://b.com");
    }

    #[test]
    fn detect_url_with_path() {
        let urls = detect_urls_in_line("See https://example.com/path/to/page?query=1#anchor", 0);
        assert_eq!(urls.len(), 1);
        assert!(urls[0].url.contains("/path/to/page"));
    }

    #[test]
    fn trim_trailing_punctuation() {
        let urls = detect_urls_in_line("Link: https://example.com.", 0);
        assert_eq!(urls.len(), 1);
        assert_eq!(urls[0].url, "https://example.com");
    }

    #[test]
    fn url_at_position_test() {
        let urls = detect_urls_in_line("Visit https://example.com now", 0);
        assert!(url_at_position(&urls, 15, 0).is_some());
        assert!(url_at_position(&urls, 0, 0).is_none());
    }
}
