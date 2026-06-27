//! Utility functions matching `gutils.h` / `gutils.c` (partial).
//!
//! Only the platform-independent parts are implemented here:
//! - `get_prgname` / `set_prgname`
//! - `get_application_name` / `set_application_name`
//! - OS info key constants
//!
//! OS-dependent functions (home dir, tmp dir, user name, host name)
//! are deferred to a platform abstraction layer.
//! Fully `no_std` compatible using `alloc` and `spin`.

use crate::prelude::*;
use spin::Mutex;

static PRGNAME: Mutex<Option<String>> = Mutex::new(None);
static APP_NAME: Mutex<Option<String>> = Mutex::new(None);

/// Get the program name (`g_get_prgname`).
pub fn get_prgname() -> Option<String> {
    PRGNAME.lock().clone()
}

/// Set the program name (`g_set_prgname`).
pub fn set_prgname(name: &str) {
    *PRGNAME.lock() = Some(name.to_owned());
}

/// Get the application name (`g_get_application_name`).
pub fn get_application_name() -> Option<String> {
    APP_NAME.lock().clone()
}

/// Set the application name (`g_set_application_name`).
pub fn set_application_name(name: &str) {
    *APP_NAME.lock() = Some(name.to_owned());
}

/// OS info key: name (`G_OS_INFO_KEY_NAME`).
pub const OS_INFO_KEY_NAME: &str = "NAME";
/// OS info key: pretty name (`G_OS_INFO_KEY_PRETTY_NAME`).
pub const OS_INFO_KEY_PRETTY_NAME: &str = "PRETTY_NAME";
/// OS info key: version (`G_OS_INFO_KEY_VERSION`).
pub const OS_INFO_KEY_VERSION: &str = "VERSION";
/// OS info key: version codename (`G_OS_INFO_KEY_VERSION_CODENAME`).
pub const OS_INFO_KEY_VERSION_CODENAME: &str = "VERSION_CODENAME";
/// OS info key: version ID (`G_OS_INFO_KEY_VERSION_ID`).
pub const OS_INFO_KEY_VERSION_ID: &str = "VERSION_ID";
/// OS info key: ID (`G_OS_INFO_KEY_ID`).
pub const OS_INFO_KEY_ID: &str = "ID";
/// OS info key: home URL (`G_OS_INFO_KEY_HOME_URL`).
pub const OS_INFO_KEY_HOME_URL: &str = "HOME_URL";
/// OS info key: documentation URL (`G_OS_INFO_KEY_DOCUMENTATION_URL`).
pub const OS_INFO_KEY_DOCUMENTATION_URL: &str = "DOCUMENTATION_URL";
/// OS info key: support URL (`G_OS_INFO_KEY_SUPPORT_URL`).
pub const OS_INFO_KEY_SUPPORT_URL: &str = "SUPPORT_URL";
/// OS info key: bug report URL (`G_OS_INFO_KEY_BUG_REPORT_URL`).
pub const OS_INFO_KEY_BUG_REPORT_URL: &str = "BUG_REPORT_URL";
/// OS info key: privacy policy URL (`G_OS_INFO_KEY_PRIVACY_POLICY_URL`).
pub const OS_INFO_KEY_PRIVACY_POLICY_URL: &str = "PRIVACY_POLICY_URL";

/// Microseconds per second.
pub const USEC_PER_SEC: u32 = 1_000_000;
/// Nanoseconds per second.
pub const NSEC_PER_SEC: u64 = 1_000_000_000;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prgname() {
        set_prgname("test_program");
        assert_eq!(get_prgname(), Some("test_program".to_owned()));
    }

    #[test]
    fn application_name() {
        set_application_name("TestApp");
        assert_eq!(get_application_name(), Some("TestApp".to_owned()));
    }

    #[test]
    fn os_info_keys() {
        assert_eq!(OS_INFO_KEY_NAME, "NAME");
        assert_eq!(OS_INFO_KEY_PRETTY_NAME, "PRETTY_NAME");
        assert_eq!(OS_INFO_KEY_VERSION, "VERSION");
    }
}
