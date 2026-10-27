use serde::{Deserialize, Serialize};

const TIMEOUT_MS: (u32, u32) = (1_000, 120_000);

/// A request to any web address, as a key sends it. Everything, headers included, is stored as
/// it is in the configuration.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, specta::Type)]
pub struct HttpRequest {
    pub method: HttpMethod,
    pub url: String,
    #[serde(default)]
    pub headers: Vec<HttpPair>,
    #[serde(default)]
    pub body: HttpBody,
    #[serde(default = "default_timeout")]
    pub timeout_ms: u32,
    /// Accepts certificates that cannot be verified, as devices on the local network often have.
    #[serde(default)]
    pub insecure: bool,
    /// The key shows for a moment whether the request worked.
    #[serde(default = "yes")]
    pub show_result: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "UPPERCASE")]
pub enum HttpMethod {
    Get,
    Post,
    Put,
    Patch,
    Delete,
    Head,
    Options,
}

/// A header, or a field of a form.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
pub struct HttpPair {
    pub name: String,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, specta::Type)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum HttpBody {
    #[default]
    None,
    Json {
        json: String,
    },
    Text {
        text: String,
        content_type: String,
    },
    /// Sent as `application/x-www-form-urlencoded`.
    Form {
        fields: Vec<HttpPair>,
    },
}

fn default_timeout() -> u32 {
    10_000
}

fn yes() -> bool {
    true
}

impl HttpMethod {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Get => "GET",
            Self::Post => "POST",
            Self::Put => "PUT",
            Self::Patch => "PATCH",
            Self::Delete => "DELETE",
            Self::Head => "HEAD",
            Self::Options => "OPTIONS",
        }
    }
}

impl HttpRequest {
    /// What keeps the request from being sent, the same checks the editor makes.
    pub fn problem(&self) -> Option<&'static str> {
        if !is_web_url(self.url.trim()) {
            return Some("the address must start with http:// or https://");
        }
        if self.headers.iter().any(|h| !is_header_name(&h.name)) {
            return Some("a header name has spaces or symbols it cannot have");
        }
        if self
            .headers
            .iter()
            .any(|h| h.value.chars().any(|c| c == '\r' || c == '\n'))
        {
            return Some("a header value cannot span several lines");
        }
        if !(TIMEOUT_MS.0..=TIMEOUT_MS.1).contains(&self.timeout_ms) {
            return Some("the time limit goes from 1 to 120 seconds");
        }
        match &self.body {
            HttpBody::Json { json } if serde_json::from_str::<serde_json::Value>(json).is_err() => {
                Some("the body is not valid JSON")
            }
            HttpBody::Text { content_type, .. } if content_type.trim().is_empty() => {
                Some("say what type of text the body is")
            }
            HttpBody::Form { fields } if fields.iter().any(|f| f.name.trim().is_empty()) => {
                Some("every form field needs a name")
            }
            _ => None,
        }
    }
}

fn is_web_url(url: &str) -> bool {
    let rest = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"));
    matches!(rest, Some(host) if !host.is_empty() && !host.starts_with('/'))
        && !url.chars().any(|c| c.is_whitespace() || c.is_control())
}

/// The characters RFC 9110 allows in a field name.
fn is_header_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "!#$%&'*+-.^_`|~".contains(c))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(url: &str) -> HttpRequest {
        HttpRequest {
            method: HttpMethod::Post,
            url: url.into(),
            headers: Vec::new(),
            body: HttpBody::None,
            timeout_ms: 10_000,
            insecure: false,
            show_result: true,
        }
    }

    #[test]
    fn reads_a_minimal_request_with_defaults() {
        let parsed: HttpRequest =
            serde_json::from_str(r#"{"method":"GET","url":"https://example.com"}"#).unwrap();
        assert_eq!(parsed.method, HttpMethod::Get);
        assert!(parsed.headers.is_empty());
        assert_eq!(parsed.body, HttpBody::None);
        assert_eq!(parsed.timeout_ms, 10_000);
        assert!(!parsed.insecure);
        assert!(parsed.show_result);
    }

    #[test]
    fn round_trips_every_body() {
        for body in [
            HttpBody::None,
            HttpBody::Json {
                json: r#"{"a":1}"#.into(),
            },
            HttpBody::Text {
                text: "hola".into(),
                content_type: "text/plain".into(),
            },
            HttpBody::Form {
                fields: vec![HttpPair {
                    name: "a".into(),
                    value: "b".into(),
                }],
            },
        ] {
            let mut original = request("https://example.com");
            original.body = body;
            let json = serde_json::to_string(&original).unwrap();
            assert_eq!(
                serde_json::from_str::<HttpRequest>(&json).unwrap(),
                original
            );
        }
        assert_eq!(
            serde_json::to_string(&HttpMethod::Delete).unwrap(),
            r#""DELETE""#
        );
    }

    #[test]
    fn accepts_only_web_addresses() {
        assert!(request("https://example.com/api?x=1").problem().is_none());
        assert!(request("  http://192.168.1.5:8123/  ").problem().is_none());
        for url in [
            "",
            "ftp://example.com",
            "https://",
            "https:///path",
            "https://a b.com",
        ] {
            assert!(request(url).problem().is_some(), "{url}");
        }
    }

    #[test]
    fn checks_headers_body_and_time_limit() {
        let header = |name: &str, value: &str| {
            let mut r = request("https://example.com");
            r.headers.push(HttpPair {
                name: name.into(),
                value: value.into(),
            });
            r
        };
        assert!(header("X-API-Key", "abc").problem().is_none());
        assert!(header("", "abc").problem().is_some());
        assert!(header("Bad Name", "abc").problem().is_some());
        assert!(header("X-A", "one\ntwo").problem().is_some());

        let body = |body: HttpBody| {
            let mut r = request("https://example.com");
            r.body = body;
            r.problem()
        };
        assert!(body(HttpBody::Json { json: "{".into() }).is_some());
        assert!(
            body(HttpBody::Text {
                text: "x".into(),
                content_type: " ".into()
            })
            .is_some()
        );
        assert!(
            body(HttpBody::Form {
                fields: vec![HttpPair {
                    name: "".into(),
                    value: "x".into()
                }]
            })
            .is_some()
        );

        let mut slow = request("https://example.com");
        slow.timeout_ms = 500;
        assert!(slow.problem().is_some());
        slow.timeout_ms = 120_001;
        assert!(slow.problem().is_some());
    }
}
