//! URI parsing and construction matching `guri.h` / `guri.c`.
//!
//! Implements RFC 3986 URI parsing, building, and normalization.
//! Fully `no_std` compatible using `alloc`.

#![allow(missing_docs)]

use crate::prelude::*;

/// URI flags (`GUriFlags`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UriFlags(pub u32);

impl UriFlags {
    pub const NONE: Self = Self(0);
    pub const PARSE_RELAXED: Self = Self(1 << 0);
    pub const HAS_PASSWORD: Self = Self(1 << 1);
    pub const HAS_AUTH_PARAMS: Self = Self(1 << 2);
    pub const ENCODED: Self = Self(1 << 3);
    pub const NON_DNS: Self = Self(1 << 4);
    pub const ENCODED_QUERY: Self = Self(1 << 5);
    pub const ENCODED_PATH: Self = Self(1 << 6);
    pub const ENCODED_FRAGMENT: Self = Self(1 << 7);
    pub const SCHEME_NORMALIZE: Self = Self(1 << 8);
}

/// Flags for hiding parts of a URI (`GUriHideFlags`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UriHideFlags(pub u32);

impl UriHideFlags {
    pub const NONE: Self = Self(0);
    pub const USERINFO: Self = Self(1 << 0);
    pub const PASSWORD: Self = Self(1 << 1);
    pub const AUTH_PARAMS: Self = Self(1 << 2);
    pub const QUERY: Self = Self(1 << 3);
    pub const FRAGMENT: Self = Self(1 << 4);
}

impl core::ops::BitOr for UriHideFlags {
    type Output = Self;
    fn bitor(self, rhs: Self) -> Self {
        Self(self.0 | rhs.0)
    }
}

impl core::ops::BitOrAssign for UriHideFlags {
    fn bitor_assign(&mut self, rhs: Self) {
        self.0 |= rhs.0;
    }
}

/// A parsed URI (`GUri`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Uri {
    pub scheme: String,
    pub userinfo: Option<String>,
    pub host: String,
    pub port: Option<u16>,
    pub path: String,
    pub query: Option<String>,
    pub fragment: Option<String>,
    pub flags: UriFlags,
}

impl Uri {
    /// Parse a URI string (`g_uri_parse`).
    pub fn parse(uri_string: &str, flags: UriFlags) -> Result<Self, UriError> {
        let parsed = parse_uri(uri_string, flags)?;
        Ok(parsed)
    }

    /// Build a URI from components (`g_uri_build`).
    #[allow(clippy::too_many_arguments)]
    pub fn build(
        flags: UriFlags,
        scheme: &str,
        userinfo: Option<&str>,
        host: &str,
        port: Option<u16>,
        path: &str,
        query: Option<&str>,
        fragment: Option<&str>,
    ) -> Self {
        Self {
            scheme: scheme.to_owned(),
            userinfo: userinfo.map(|s| s.to_owned()),
            host: host.to_owned(),
            port,
            path: path.to_owned(),
            query: query.map(|s| s.to_owned()),
            fragment: fragment.map(|s| s.to_owned()),
            flags,
        }
    }

    /// Convert to string with parts hidden (`g_uri_to_string_partial`).
    pub fn to_string_partial(&self, hide: UriHideFlags) -> String {
        let mut result = String::new();
        result.push_str(&self.scheme);
        result.push(':');

        if !self.host.is_empty() || self.userinfo.is_some() || self.port.is_some() {
            result.push_str("//");
            if let Some(ref ui) = self.userinfo {
                if hide.0 & UriHideFlags::USERINFO.0 == 0 {
                    result.push_str(ui);
                    result.push('@');
                }
            }
            result.push_str(&self.host);
            if let Some(port) = self.port {
                result.push(':');
                let _ = write!(result, "{}", port);
            }
        }

        result.push_str(&self.path);

        if let Some(ref q) = self.query {
            if hide.0 & UriHideFlags::QUERY.0 == 0 {
                result.push('?');
                result.push_str(q);
            }
        }

        if let Some(ref f) = self.fragment {
            if hide.0 & UriHideFlags::FRAGMENT.0 == 0 {
                result.push('#');
                result.push_str(f);
            }
        }

        result
    }

    /// Get the scheme.
    pub fn scheme(&self) -> &str {
        &self.scheme
    }

    /// Get the userinfo.
    pub fn userinfo(&self) -> Option<&str> {
        self.userinfo.as_deref()
    }

    /// Get the host.
    pub fn host(&self) -> &str {
        &self.host
    }

    /// Get the port.
    pub fn port(&self) -> Option<u16> {
        self.port
    }

    /// Get the path.
    pub fn path(&self) -> &str {
        &self.path
    }

    /// Get the query.
    pub fn query(&self) -> Option<&str> {
        self.query.as_deref()
    }

    /// Get the fragment.
    pub fn fragment(&self) -> Option<&str> {
        self.fragment.as_deref()
    }
}

impl core::fmt::Display for Uri {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(&self.to_string_partial(UriHideFlags::NONE))
    }
}

/// URI error codes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum UriError {
    /// Malformed URI.
    BadUri,
    /// Invalid scheme.
    BadScheme,
    /// Invalid user info.
    BadUserInfo,
    /// Invalid host.
    BadHost,
    /// Invalid port.
    BadPort,
    /// Invalid path.
    BadPath,
    /// Invalid query.
    BadQuery,
    /// Invalid fragment.
    BadFragment,
}

/// Parse a URI string into components.
fn parse_uri(uri_string: &str, _flags: UriFlags) -> Result<Uri, UriError> {
    let s = uri_string.as_bytes();
    if s.is_empty() {
        return Err(UriError::BadUri);
    }

    // Parse scheme: ALPHA *( ALPHA / DIGIT / "+" / "-" / "." ) ":"
    let mut scheme_end = 0;
    if !s[0].is_ascii_alphabetic() {
        return Err(UriError::BadScheme);
    }
    while scheme_end < s.len() && s[scheme_end] != b':' {
        if !s[scheme_end].is_ascii_alphanumeric()
            && s[scheme_end] != b'+'
            && s[scheme_end] != b'-'
            && s[scheme_end] != b'.'
        {
            return Err(UriError::BadScheme);
        }
        scheme_end += 1;
    }
    if scheme_end >= s.len() {
        return Err(UriError::BadScheme);
    }

    let scheme = core::str::from_utf8(&s[..scheme_end])
        .map_err(|_| UriError::BadScheme)?
        .to_ascii_lowercase();
    let mut pos = scheme_end + 1; // skip ':'

    // Parse authority: "//" userinfo "@" host ":" port
    let mut userinfo: Option<String> = None;
    let mut host = String::new();
    let mut port: Option<u16> = None;

    if pos + 1 < s.len() && s[pos] == b'/' && s[pos + 1] == b'/' {
        pos += 2;
        let authority_start = pos;

        // Find end of authority (next / ? or #)
        let authority_end = s[pos..]
            .iter()
            .position(|&b| b == b'/' || b == b'?' || b == b'#')
            .map(|p| pos + p)
            .unwrap_or(s.len());
        let authority = &s[authority_start..authority_end];

        // Split userinfo@host:port
        let (auth_host_part, auth_userinfo) = if let Some(at_pos) = authority.iter().position(|&b| b == b'@') {
            let ui = &authority[..at_pos];
            let rest = &authority[at_pos + 1..];
            (rest, Some(core::str::from_utf8(ui).map_err(|_| UriError::BadUserInfo)?.to_owned()))
        } else {
            (authority, None)
        };
        userinfo = auth_userinfo;

        // Split host:port
        if auth_host_part.starts_with(b"[") {
            // IPv6 literal: [addr]:port
            if let Some(bracket_end) = auth_host_part.iter().position(|&b| b == b']') {
                host = core::str::from_utf8(&auth_host_part[..=bracket_end])
                    .map_err(|_| UriError::BadHost)?
                    .to_owned();
                let after = &auth_host_part[bracket_end + 1..];
                if !after.is_empty() && after[0] == b':' {
                    let port_str = core::str::from_utf8(&after[1..])
                        .map_err(|_| UriError::BadPort)?;
                    port = Some(port_str.parse::<u16>().map_err(|_| UriError::BadPort)?);
                }
            } else {
                return Err(UriError::BadHost);
            }
        } else {
            // Regular host:port
            let (h, p) = if let Some(colon) = auth_host_part.iter().rposition(|&b| b == b':') {
                let h = core::str::from_utf8(&auth_host_part[..colon])
                    .map_err(|_| UriError::BadHost)?;
                let p_str = core::str::from_utf8(&auth_host_part[colon + 1..])
                    .map_err(|_| UriError::BadPort)?;
                let port_num = if p_str.is_empty() {
                    None
                } else {
                    Some(p_str.parse::<u16>().map_err(|_| UriError::BadPort)?)
                };
                (h.to_owned(), port_num)
            } else {
                (
                    core::str::from_utf8(auth_host_part)
                        .map_err(|_| UriError::BadHost)?
                        .to_owned(),
                    None,
                )
            };
            host = h;
            port = p;
        }

        pos = authority_end;
    }

    // Parse path
    let path_end = s[pos..]
        .iter()
        .position(|&b| b == b'?' || b == b'#')
        .map(|p| pos + p)
        .unwrap_or(s.len());
    let path = core::str::from_utf8(&s[pos..path_end])
        .map_err(|_| UriError::BadPath)?
        .to_owned();
    pos = path_end;

    // Parse query
    let mut query: Option<String> = None;
    if pos < s.len() && s[pos] == b'?' {
        pos += 1;
        let query_end = s[pos..]
            .iter()
            .position(|&b| b == b'#')
            .map(|p| pos + p)
            .unwrap_or(s.len());
        query = Some(
            core::str::from_utf8(&s[pos..query_end])
                .map_err(|_| UriError::BadQuery)?
                .to_owned(),
        );
        pos = query_end;
    }

    // Parse fragment
    let mut fragment: Option<String> = None;
    if pos < s.len() && s[pos] == b'#' {
        pos += 1;
        fragment = Some(
            core::str::from_utf8(&s[pos..])
                .map_err(|_| UriError::BadFragment)?
                .to_owned(),
        );
    }

    Ok(Uri {
        scheme,
        userinfo,
        host,
        port,
        path,
        query,
        fragment,
        flags: _flags,
    })
}

/// Peek the scheme of a URI string (`g_uri_peek_scheme`).
pub fn peek_scheme(uri: &str) -> Option<String> {
    let s = uri.as_bytes();
    if s.is_empty() || !s[0].is_ascii_alphabetic() {
        return None;
    }
    let end = s.iter().position(|&b| b == b':')?;
    let scheme = core::str::from_utf8(&s[..end]).ok()?;
    if scheme.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'+' || b == b'-' || b == b'.') {
        Some(scheme.to_ascii_lowercase())
    } else {
        None
    }
}

/// Join URI components into a string (`g_uri_join`).
#[allow(clippy::too_many_arguments)]
pub fn join(
    flags: UriFlags,
    scheme: &str,
    userinfo: Option<&str>,
    host: &str,
    port: Option<u16>,
    path: &str,
    query: Option<&str>,
    fragment: Option<&str>,
) -> String {
    Uri::build(flags, scheme, userinfo, host, port, path, query, fragment).to_string()
}

/// Check if a URI string is valid (`g_uri_is_valid`).
pub fn is_valid(uri_string: &str, flags: UriFlags) -> bool {
    Uri::parse(uri_string, flags).is_ok()
}

/// Percent-encode a string for URI use.
pub fn escape_string(s: &str, allow: &str) -> String {
    let mut result = String::with_capacity(s.len());
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || allow.as_bytes().contains(&b) || matches!(b, b'-' | b'_' | b'.' | b'~') {
            result.push(b as char);
        } else {
            let _ = write!(result, "%{:02X}", b);
        }
    }
    result
}

/// Percent-decode a string.
pub fn unescape_string(s: &str) -> Result<String, UriError> {
    let bytes = s.as_bytes();
    let mut result: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            if i + 2 >= bytes.len() {
                return Err(UriError::BadUri);
            }
            let h = hex_val(bytes[i + 1]).ok_or(UriError::BadUri)?;
            let l = hex_val(bytes[i + 2]).ok_or(UriError::BadUri)?;
            result.push((h << 4) | l);
            i += 3;
        } else {
            result.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(result).map_err(|_| UriError::BadUri)
}

fn hex_val(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_http_url() {
        let uri = Uri::parse("http://example.com/path?q=1#frag", UriFlags::NONE).unwrap();
        assert_eq!(uri.scheme(), "http");
        assert_eq!(uri.host(), "example.com");
        assert_eq!(uri.path(), "/path");
        assert_eq!(uri.query(), Some("q=1"));
        assert_eq!(uri.fragment(), Some("frag"));
        assert_eq!(uri.port(), None);
    }

    #[test]
    fn parse_with_port() {
        let uri = Uri::parse("http://example.com:8080/path", UriFlags::NONE).unwrap();
        assert_eq!(uri.port(), Some(8080));
        assert_eq!(uri.path(), "/path");
    }

    #[test]
    fn parse_with_userinfo() {
        let uri = Uri::parse("ftp://user@host/path", UriFlags::NONE).unwrap();
        assert_eq!(uri.userinfo(), Some("user"));
        assert_eq!(uri.host(), "host");
    }

    #[test]
    fn parse_ipv6() {
        let uri = Uri::parse("http://[::1]:8080/path", UriFlags::NONE).unwrap();
        assert_eq!(uri.host(), "[::1]");
        assert_eq!(uri.port(), Some(8080));
    }

    #[test]
    fn parse_simple_scheme() {
        let uri = Uri::parse("mailto:test@example.com", UriFlags::NONE).unwrap();
        assert_eq!(uri.scheme(), "mailto");
        assert_eq!(uri.path(), "test@example.com");
        assert_eq!(uri.host(), "");
    }

    #[test]
    fn parse_no_authority() {
        let uri = Uri::parse("urn:isbn:1234567890", UriFlags::NONE).unwrap();
        assert_eq!(uri.scheme(), "urn");
        assert_eq!(uri.path(), "isbn:1234567890");
    }

    #[test]
    fn to_string_roundtrip() {
        let uri = Uri::parse("http://user@example.com:8080/path?q=1#f", UriFlags::NONE).unwrap();
        let s = uri.to_string();
        assert_eq!(s, "http://user@example.com:8080/path?q=1#f");
    }

    #[test]
    fn to_string_partial() {
        let uri = Uri::parse("http://user@example.com/path?q=1#f", UriFlags::NONE).unwrap();
        let s = uri.to_string_partial(UriHideFlags::USERINFO | UriHideFlags::FRAGMENT);
        assert_eq!(s, "http://example.com/path?q=1");
    }

    #[test]
    fn peek_scheme() {
        assert_eq!(super::peek_scheme("http://example.com"), Some("http".to_owned()));
        assert_eq!(super::peek_scheme("mailto:test"), Some("mailto".to_owned()));
        assert_eq!(super::peek_scheme("not a uri"), None);
    }

    #[test]
    fn escape_and_unescape() {
        let original = "hello world/test";
        let escaped = escape_string(original, "/");
        assert_eq!(escaped, "hello%20world/test");
        let decoded = unescape_string(&escaped).unwrap();
        assert_eq!(decoded, original);
    }

    #[test]
    fn build_uri() {
        let uri = Uri::build(
            UriFlags::NONE,
            "https",
            None,
            "example.com",
            Some(443),
            "/api",
            Some("q=test"),
            None,
        );
        assert_eq!(uri.to_string(), "https://example.com:443/api?q=test");
    }

    #[test]
    fn invalid_uris() {
        assert!(Uri::parse("", UriFlags::NONE).is_err());
        assert!(Uri::parse("1bad://host", UriFlags::NONE).is_err());
        assert!(Uri::parse("http://host:99999", UriFlags::NONE).is_err());
    }
}
