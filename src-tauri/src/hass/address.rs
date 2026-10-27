//! Where Home Assistant is, from the address the user types as the browser would open it.

const DEFAULT_PORT: u16 = 8123;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Address {
    pub secure: bool,
    pub host: String,
    pub port: u16,
}

impl Address {
    /// `homeassistant.local`, `192.168.1.10:8123`, `http://…:8123/lovelace/0` or
    /// `https://…ui.nabu.casa`. Without a scheme it is `http` on Home Assistant's own port; with
    /// one, the scheme's port. Whatever path follows is the browser's, not the API's.
    pub fn parse(url: &str) -> Option<Self> {
        let url = url.trim();
        let (scheme, rest) = match url.split_once("://") {
            Some((scheme, rest)) => (Some(scheme.to_ascii_lowercase()), rest),
            None => (None, url),
        };
        let secure = match scheme.as_deref() {
            None | Some("http" | "ws") => false,
            Some("https" | "wss") => true,
            Some(_) => return None,
        };
        let authority = rest.split(['/', '?', '#']).next().unwrap_or_default();
        let authority = authority.rsplit('@').next().unwrap_or_default();
        let (host, port) = if let Some(bracketed) = authority.strip_prefix('[') {
            let (host, after) = bracketed.split_once(']')?;
            (host.to_string(), after.strip_prefix(':'))
        } else {
            match authority.rsplit_once(':') {
                Some((host, port)) => (host.to_string(), Some(port)),
                None => (authority.to_string(), None),
            }
        };
        if host.is_empty() || host.contains(char::is_whitespace) {
            return None;
        }
        let port = match port {
            Some(port) => port.parse().ok().filter(|&p| p != 0)?,
            None if scheme.is_none() => DEFAULT_PORT,
            None if secure => 443,
            None => 80,
        };
        Some(Self {
            secure,
            host: host.to_ascii_lowercase(),
            port,
        })
    }

    fn authority(&self) -> String {
        let host = if self.host.contains(':') {
            format!("[{}]", self.host)
        } else {
            self.host.clone()
        };
        let default = if self.secure { 443 } else { 80 };
        if self.port == default {
            host
        } else {
            format!("{host}:{}", self.port)
        }
    }

    pub fn websocket(&self) -> String {
        let scheme = if self.secure { "wss" } else { "ws" };
        format!("{scheme}://{}/api/websocket", self.authority())
    }

    /// The web address of `path` on this Home Assistant, as its pages link it.
    pub fn web(&self, path: &str) -> String {
        format!("{}{path}", self.origin())
    }

    pub fn origin(&self) -> String {
        let scheme = if self.secure { "https" } else { "http" };
        format!("{scheme}://{}", self.authority())
    }

    /// Whether `url` points at this Home Assistant, which is the only place the token goes.
    pub fn owns(&self, url: &str) -> bool {
        let origin = self.origin();
        url.strip_prefix(&origin)
            .is_some_and(|rest| rest.is_empty() || rest.starts_with('/'))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(url: &str) -> Option<(bool, String, u16)> {
        Address::parse(url).map(|a| (a.secure, a.host, a.port))
    }

    #[test]
    fn reads_addresses_as_typed() {
        assert_eq!(
            parse("homeassistant.local"),
            Some((false, "homeassistant.local".into(), 8123))
        );
        assert_eq!(
            parse(" 192.168.1.10:8124 "),
            Some((false, "192.168.1.10".into(), 8124))
        );
        assert_eq!(
            parse("http://HA.local:8123/lovelace/0?edit=1"),
            Some((false, "ha.local".into(), 8123))
        );
        assert_eq!(
            parse("https://abc.ui.nabu.casa/"),
            Some((true, "abc.ui.nabu.casa".into(), 443))
        );
        assert_eq!(
            parse("http://ha.local"),
            Some((false, "ha.local".into(), 80))
        );
        assert_eq!(
            parse("http://[::1]:8123"),
            Some((false, "::1".into(), 8123))
        );
        for bad in [
            "",
            "ftp://ha",
            "http://",
            "http://ha:0",
            "http://ha:x",
            "ha local",
        ] {
            assert_eq!(parse(bad), None, "{bad}");
        }
    }

    #[test]
    fn builds_the_api_and_web_addresses() {
        let local = Address::parse("ha.local").unwrap();
        assert_eq!(local.websocket(), "ws://ha.local:8123/api/websocket");
        assert_eq!(
            local.web("/api/camera_proxy/camera.door"),
            "http://ha.local:8123/api/camera_proxy/camera.door"
        );
        let cloud = Address::parse("https://abc.ui.nabu.casa").unwrap();
        assert_eq!(cloud.websocket(), "wss://abc.ui.nabu.casa/api/websocket");
        let v6 = Address::parse("http://[fe80::1]:8123").unwrap();
        assert_eq!(v6.websocket(), "ws://[fe80::1]:8123/api/websocket");
    }

    #[test]
    fn the_token_only_goes_to_home_assistant() {
        let address = Address::parse("http://ha.local:8123").unwrap();
        assert!(address.owns("http://ha.local:8123/api/image_proxy/image.x"));
        assert!(!address.owns("http://ha.local:81234/x"));
        assert!(!address.owns("http://ha.local:8123.evil.com/x"));
        assert!(!address.owns("https://i.scdn.co/image/abc"));
    }
}
