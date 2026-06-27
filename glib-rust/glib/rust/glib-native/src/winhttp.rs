//! `winhttp` matching `gio/win32/winhttp.h`.
//!
//! WinHTTP API constants, types, and function signatures.
//! In our no_std port, WinHTTP functions are stubs since we don't
//! have access to the Windows WinHTTP DLL.
//!
//! Fully `no_std` compatible using `alloc`.

use alloc::string::{String, ToString};
use alloc::vec::Vec;

/// WinHTTP handle type (mirrors `HINTERNET`).
pub type HInternet = usize;

/// Internet port type (mirrors `INTERNET_PORT`).
pub type InternetPort = u16;

/// Internet scheme type (mirrors `INTERNET_SCHEME`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InternetScheme {
    Http = 1,
    Https = 2,
}

/// URL components (mirrors `URL_COMPONENTS`).
#[derive(Debug, Clone, Default)]
pub struct UrlComponents {
    pub scheme: String,
    pub scheme_type: Option<InternetScheme>,
    pub host_name: String,
    pub port: InternetPort,
    pub user_name: String,
    pub password: String,
    pub url_path: String,
    pub extra_info: String,
}

/// WinHTTP async result (mirrors `WINHTTP_ASYNC_RESULT`).
#[derive(Debug, Clone)]
pub struct WinHttpAsyncResult {
    pub result: usize,
    pub error: u32,
}

/// WinHTTP certificate info (mirrors `WINHTTP_CERTIFICATE_INFO`).
#[derive(Debug, Clone, Default)]
pub struct WinHttpCertificateInfo {
    pub expiry: u64,
    pub start: u64,
    pub subject_info: String,
    pub issuer_info: String,
    pub protocol_name: String,
    pub signature_alg_name: String,
    pub encryption_alg_name: String,
    pub key_size: u32,
}

/// WinHTTP proxy info (mirrors `WINHTTP_PROXY_INFO`).
#[derive(Debug, Clone, Default)]
pub struct WinHttpProxyInfo {
    pub access_type: u32,
    pub proxy: String,
    pub proxy_bypass: String,
}

/// WinHTTP current user IE proxy config (mirrors `WINHTTP_CURRENT_USER_IE_PROXY_CONFIG`).
#[derive(Debug, Clone, Default)]
pub struct WinHttpCurrentUserIeProxyConfig {
    pub auto_detect: bool,
    pub auto_config_url: String,
    pub proxy: String,
    pub proxy_bypass: String,
}

/// WinHTTP autoproxy options (mirrors `WINHTTP_AUTOPROXY_OPTIONS`).
#[derive(Debug, Clone, Default)]
pub struct WinHttpAutoproxyOptions {
    pub flags: u32,
    pub auto_detect_flags: u32,
    pub auto_config_url: String,
    pub auto_logon_if_challenged: bool,
}

/// HTTP version info (mirrors `HTTP_VERSION_INFO`).
#[derive(Debug, Clone, Default)]
pub struct HttpVersionInfo {
    pub major_version: u32,
    pub minor_version: u32,
}

// ── Constants ──────────────────────────────────────────────────────────────

pub const INTERNET_DEFAULT_PORT: InternetPort = 0;
pub const INTERNET_DEFAULT_HTTP_PORT: InternetPort = 80;
pub const INTERNET_DEFAULT_HTTPS_PORT: InternetPort = 443;

pub const WINHTTP_FLAG_ASYNC: u32 = 0x10000000;
pub const WINHTTP_FLAG_ESCAPE_PERCENT: u32 = 0x00000004;
pub const WINHTTP_FLAG_NULL_CODEPAGE: u32 = 0x00000008;
pub const WINHTTP_FLAG_ESCAPE_DISABLE: u32 = 0x00000040;
pub const WINHTTP_FLAG_ESCAPE_DISABLE_QUERY: u32 = 0x00000080;
pub const WINHTTP_FLAG_BYPASS_PROXY_CACHE: u32 = 0x00000100;
pub const WINHTTP_FLAG_REFRESH: u32 = WINHTTP_FLAG_BYPASS_PROXY_CACHE;
pub const WINHTTP_FLAG_SECURE: u32 = 0x00800000;

pub const WINHTTP_ACCESS_TYPE_DEFAULT_PROXY: u32 = 0;
pub const WINHTTP_ACCESS_TYPE_NO_PROXY: u32 = 1;
pub const WINHTTP_ACCESS_TYPE_NAMED_PROXY: u32 = 3;

pub const WINHTTP_ERROR_BASE: u32 = 12000;

pub const ERROR_WINHTTP_CANNOT_CONNECT: u32 = 12029;
pub const ERROR_WINHTTP_CONNECTION_ERROR: u32 = 12030;
pub const ERROR_WINHTTP_HEADER_NOT_FOUND: u32 = 12150;
pub const ERROR_WINHTTP_INCORRECT_HANDLE_STATE: u32 = 12019;
pub const ERROR_WINHTTP_INCORRECT_HANDLE_TYPE: u32 = 12018;
pub const ERROR_WINHTTP_INTERNAL_ERROR: u32 = 12004;
pub const ERROR_WINHTTP_INVALID_URL: u32 = 12005;
pub const ERROR_WINHTTP_LOGIN_FAILURE: u32 = 12015;
pub const ERROR_WINHTTP_NAME_NOT_RESOLVED: u32 = 12007;
pub const ERROR_WINHTTP_OPERATION_CANCELLED: u32 = 12017;
pub const ERROR_WINHTTP_OUT_OF_HANDLES: u32 = 12001;
pub const ERROR_WINHTTP_TIMEOUT: u32 = 12002;
pub const ERROR_WINHTTP_UNRECOGNIZED_SCHEME: u32 = 12006;

pub const WINHTTP_QUERY_CONTENT_LENGTH: u32 = 5;
pub const WINHTTP_QUERY_CONTENT_TYPE: u32 = 1;
pub const WINHTTP_QUERY_LAST_MODIFIED: u32 = 11;
pub const WINHTTP_QUERY_STATUS_CODE: u32 = 19;
pub const WINHTTP_QUERY_STATUS_TEXT: u32 = 20;

pub const WINHTTP_QUERY_FLAG_SYSTEMTIME: u32 = 0x40000000;

pub const ICU_ESCAPE: u32 = 0x80000000;
pub const ICU_DECODE: u32 = 0x10000000;

/// Parses a URL into components (mirrors `WinHttpCrackUrl`).
pub fn crack_url(url: &str) -> Result<UrlComponents, String> {
    let mut components = UrlComponents::default();
    let remaining = if let Some(s) = url.strip_prefix("http://") {
        components.scheme_type = Some(InternetScheme::Http);
        components.scheme = "http".to_string();
        components.port = INTERNET_DEFAULT_HTTP_PORT;
        s
    } else if let Some(s) = url.strip_prefix("https://") {
        components.scheme_type = Some(InternetScheme::Https);
        components.scheme = "https".to_string();
        components.port = INTERNET_DEFAULT_HTTPS_PORT;
        s
    } else {
        return Err("unrecognized URL scheme".to_string());
    };
    let path_start = remaining.find('/').unwrap_or(remaining.len());
    let authority = &remaining[..path_start];
    let path = &remaining[path_start..];
    if let Some(at_pos) = authority.find('@') {
        let userinfo = &authority[..at_pos];
        let hostport = &authority[at_pos + 1..];
        if let Some(colon) = userinfo.find(':') {
            components.user_name = userinfo[..colon].to_string();
            components.password = userinfo[colon + 1..].to_string();
        } else {
            components.user_name = userinfo.to_string();
        }
        if let Some(colon) = hostport.find(':') {
            components.host_name = hostport[..colon].to_string();
            if let Ok(p) = hostport[colon + 1..].parse::<InternetPort>() {
                components.port = p;
            }
        } else {
            components.host_name = hostport.to_string();
        }
    } else if let Some(colon) = authority.find(':') {
        components.host_name = authority[..colon].to_string();
        if let Ok(p) = authority[colon + 1..].parse::<InternetPort>() {
            components.port = p;
        }
    } else {
        components.host_name = authority.to_string();
    }
    if !path.is_empty() {
        if let Some(q) = path.find('?') {
            components.url_path = path[..q].to_string();
            components.extra_info = path[q..].to_string();
        } else {
            components.url_path = path.to_string();
        }
    }
    Ok(components)
}

/// Creates a URL from components (mirrors `WinHttpCreateUrl`).
pub fn create_url(components: &UrlComponents) -> String {
    let scheme = match components.scheme_type {
        Some(InternetScheme::Https) => "https://",
        _ => "http://",
    };
    let mut url = String::from(scheme);
    if !components.user_name.is_empty() {
        url.push_str(&components.user_name);
        if !components.password.is_empty() {
            url.push(':');
            url.push_str(&components.password);
        }
        url.push('@');
    }
    url.push_str(&components.host_name);
    if components.port != 0
        && components.port != INTERNET_DEFAULT_HTTP_PORT
        && components.port != INTERNET_DEFAULT_HTTPS_PORT
    {
        url.push(':');
        url.push_str(&components.port.to_string());
    }
    url.push_str(&components.url_path);
    url.push_str(&components.extra_info);
    url
}

/// Returns a human-readable error message for a WinHTTP error code
/// (mirrors `_g_winhttp_error_message`).
pub fn error_message(error_code: u32) -> String {
    match error_code {
        ERROR_WINHTTP_OUT_OF_HANDLES => "Out of handles".to_string(),
        ERROR_WINHTTP_TIMEOUT => "Request timed out".to_string(),
        ERROR_WINHTTP_INTERNAL_ERROR => "Internal error".to_string(),
        ERROR_WINHTTP_INVALID_URL => "Invalid URL".to_string(),
        ERROR_WINHTTP_UNRECOGNIZED_SCHEME => "Unrecognized URL scheme".to_string(),
        ERROR_WINHTTP_NAME_NOT_RESOLVED => "Name not resolved".to_string(),
        ERROR_WINHTTP_CANNOT_CONNECT => "Cannot connect".to_string(),
        ERROR_WINHTTP_CONNECTION_ERROR => "Connection error".to_string(),
        ERROR_WINHTTP_INCORRECT_HANDLE_TYPE => "Incorrect handle type".to_string(),
        ERROR_WINHTTP_INCORRECT_HANDLE_STATE => "Incorrect handle state".to_string(),
        ERROR_WINHTTP_OPERATION_CANCELLED => "Operation cancelled".to_string(),
        ERROR_WINHTTP_LOGIN_FAILURE => "Login failure".to_string(),
        ERROR_WINHTTP_HEADER_NOT_FOUND => "Header not found".to_string(),
        _ => format!("WinHTTP error {}", error_code),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_crack_url_http() {
        let c = crack_url("http://example.com/path").unwrap();
        assert_eq!(c.scheme, "http");
        assert_eq!(c.host_name, "example.com");
        assert_eq!(c.port, 80);
        assert_eq!(c.url_path, "/path");
    }

    #[test]
    fn test_crack_url_https_with_port() {
        let c = crack_url("https://example.com:8443/path?query").unwrap();
        assert_eq!(c.scheme, "https");
        assert_eq!(c.host_name, "example.com");
        assert_eq!(c.port, 8443);
        assert_eq!(c.url_path, "/path");
        assert_eq!(c.extra_info, "?query");
    }

    #[test]
    fn test_crack_url_with_auth() {
        let c = crack_url("http://user:pass@example.com/").unwrap();
        assert_eq!(c.user_name, "user");
        assert_eq!(c.password, "pass");
        assert_eq!(c.host_name, "example.com");
    }

    #[test]
    fn test_create_url() {
        let mut c = UrlComponents::default();
        c.scheme_type = Some(InternetScheme::Http);
        c.host_name = "example.com".to_string();
        c.url_path = "/path".to_string();
        assert_eq!(create_url(&c), "http://example.com/path");
    }

    #[test]
    fn test_error_message() {
        assert_eq!(error_message(ERROR_WINHTTP_TIMEOUT), "Request timed out");
        assert_eq!(error_message(99999), "WinHTTP error 99999");
    }

    #[test]
    fn test_constants() {
        assert_eq!(INTERNET_DEFAULT_HTTP_PORT, 80);
        assert_eq!(INTERNET_DEFAULT_HTTPS_PORT, 443);
        assert_eq!(WINHTTP_ERROR_BASE, 12000);
        assert_eq!(WINHTTP_QUERY_STATUS_CODE, 19);
    }
}
