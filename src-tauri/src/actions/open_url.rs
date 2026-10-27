use super::error::ActionError;
use super::platform;

pub fn open_url(url: &str) -> Result<(), ActionError> {
    if !is_web_url(url) {
        return Err(ActionError::UnsupportedUrl);
    }
    platform::open(url)
}

/// Restricted to web URLs so a tampered config cannot open local files or other handlers.
fn is_web_url(url: &str) -> bool {
    let rest = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"));
    matches!(rest, Some(host) if !host.is_empty())
        && !url.chars().any(|c| c.is_whitespace() || c.is_control())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_web_urls() {
        assert!(is_web_url("https://github.com"));
        assert!(is_web_url("http://localhost:3000/path?q=1"));
    }

    #[test]
    fn rejects_other_schemes_and_odd_input() {
        assert!(!is_web_url("file:///etc/passwd"));
        assert!(!is_web_url("javascript:alert(1)"));
        assert!(!is_web_url("https://"));
        assert!(!is_web_url("https://exa mple.com"));
        assert!(!is_web_url("-a Calculator"));
    }
}
