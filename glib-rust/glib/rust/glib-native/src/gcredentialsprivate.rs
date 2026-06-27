//! `gcredentialsprivate` matching `gio/gcredentialsprivate.h`.
//!
//! Platform-specific credential type definitions. In C, these are compile-time
//! `#define`s selected by platform. In Rust, we model them as constants and
//! an enum.
//!
//! Fully `no_std` compatible.

/// Credential native type identifiers (mirrors `GCredentialsType`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CredentialsNativeType {
    Invalid,
    LinuxUcred,
    FreebsdCmsgcred,
    OpenbsdSockpeercred,
    SolarisUcred,
    NetbsdUnpcbid,
    AppleXucred,
    Win32Pid,
}

/// Platform capability flags (mirrors the `#define`s).
#[derive(Debug, Clone, Copy)]
pub struct CredentialsCapabilities {
    /// `G_CREDENTIALS_SUPPORTED`
    pub supported: bool,
    /// `G_CREDENTIALS_UNIX_CREDENTIALS_MESSAGE_SUPPORTED`
    pub message_supported: bool,
    /// `G_CREDENTIALS_SOCKET_GET_CREDENTIALS_SUPPORTED`
    pub socket_get_supported: bool,
    /// `G_CREDENTIALS_SPOOFING_SUPPORTED`
    pub spoofing_supported: bool,
    /// `G_CREDENTIALS_PREFER_MESSAGE_PASSING`
    pub prefer_message_passing: bool,
    /// `G_CREDENTIALS_HAS_PID`
    pub has_pid: bool,
}

impl Default for CredentialsCapabilities {
    fn default() -> Self {
        Self {
            supported: false,
            message_supported: false,
            socket_get_supported: false,
            spoofing_supported: false,
            prefer_message_passing: false,
            has_pid: false,
        }
    }
}

/// Target platform for credential configuration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TargetPlatform {
    Linux,
    FreeBSD,
    NetBSD,
    OpenBSD,
    Solaris,
    Apple,
    Win32,
    Unknown,
}

/// Returns the credential capabilities for the given platform.
///
/// Mirrors the `#ifdef` cascade in `gcredentialsprivate.h`.
pub fn capabilities_for_platform(
    platform: TargetPlatform,
) -> (CredentialsNativeType, CredentialsCapabilities) {
    match platform {
        TargetPlatform::Linux => (
            CredentialsNativeType::LinuxUcred,
            CredentialsCapabilities {
                supported: true,
                message_supported: true,
                socket_get_supported: true,
                spoofing_supported: true,
                prefer_message_passing: false,
                has_pid: true,
            },
        ),
        TargetPlatform::FreeBSD => (
            CredentialsNativeType::FreebsdCmsgcred,
            CredentialsCapabilities {
                supported: true,
                message_supported: true,
                socket_get_supported: false,
                spoofing_supported: true,
                prefer_message_passing: true,
                has_pid: true,
            },
        ),
        TargetPlatform::NetBSD => (
            CredentialsNativeType::NetbsdUnpcbid,
            CredentialsCapabilities {
                supported: true,
                message_supported: false,
                socket_get_supported: false,
                spoofing_supported: true,
                prefer_message_passing: false,
                has_pid: true,
            },
        ),
        TargetPlatform::OpenBSD => (
            CredentialsNativeType::OpenbsdSockpeercred,
            CredentialsCapabilities {
                supported: true,
                message_supported: false,
                socket_get_supported: true,
                spoofing_supported: true,
                prefer_message_passing: false,
                has_pid: true,
            },
        ),
        TargetPlatform::Solaris => (
            CredentialsNativeType::SolarisUcred,
            CredentialsCapabilities {
                supported: true,
                message_supported: true,
                socket_get_supported: true,
                spoofing_supported: false,
                prefer_message_passing: false,
                has_pid: true,
            },
        ),
        TargetPlatform::Apple => (
            CredentialsNativeType::AppleXucred,
            CredentialsCapabilities {
                supported: true,
                message_supported: false,
                socket_get_supported: true,
                spoofing_supported: true,
                prefer_message_passing: false,
                has_pid: false,
            },
        ),
        TargetPlatform::Win32 => (
            CredentialsNativeType::Win32Pid,
            CredentialsCapabilities {
                supported: true,
                message_supported: false,
                socket_get_supported: true,
                spoofing_supported: false,
                prefer_message_passing: false,
                has_pid: true,
            },
        ),
        TargetPlatform::Unknown => (
            CredentialsNativeType::Invalid,
            CredentialsCapabilities::default(),
        ),
    }
}

/// Sets the local peer PID on macOS credentials.
///
/// Mirrors `_g_credentials_set_local_peerid` (macOS-only).
pub fn set_local_peerid(pid: u32) -> Result<(), &'static str> {
    // On macOS this sets the xucred.cr_pid field.
    // In our no_std port, we just validate the PID.
    if pid == 0 {
        return Err("invalid PID 0");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_linux_capabilities() {
        let (native, caps) = capabilities_for_platform(TargetPlatform::Linux);
        assert_eq!(native, CredentialsNativeType::LinuxUcred);
        assert!(caps.supported);
        assert!(caps.message_supported);
        assert!(caps.socket_get_supported);
        assert!(caps.spoofing_supported);
        assert!(caps.has_pid);
        assert!(!caps.prefer_message_passing);
    }

    #[test]
    fn test_apple_capabilities() {
        let (native, caps) = capabilities_for_platform(TargetPlatform::Apple);
        assert_eq!(native, CredentialsNativeType::AppleXucred);
        assert!(caps.supported);
        assert!(!caps.has_pid);
        assert!(!caps.message_supported);
    }

    #[test]
    fn test_win32_capabilities() {
        let (native, caps) = capabilities_for_platform(TargetPlatform::Win32);
        assert_eq!(native, CredentialsNativeType::Win32Pid);
        assert!(caps.supported);
        assert!(caps.has_pid);
        assert!(!caps.message_supported);
    }

    #[test]
    fn test_unknown_capabilities() {
        let (native, caps) = capabilities_for_platform(TargetPlatform::Unknown);
        assert_eq!(native, CredentialsNativeType::Invalid);
        assert!(!caps.supported);
    }

    #[test]
    fn test_set_local_peerid() {
        assert!(set_local_peerid(1234).is_ok());
        assert!(set_local_peerid(0).is_err());
    }
}
