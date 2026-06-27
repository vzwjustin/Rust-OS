//! GMountOperation matching `gio/gmountoperation.h`.
//!
//! Upstream `GMountOperation` represents a mount operation that may
//! require user interaction (passwords, choices, etc.). We port it as
//! a struct with `Mutex`-protected credentials and state.
//!
//! Fully `no_std` compatible using `alloc`.

use alloc::string::{String, ToString};
use spin::Mutex;

/// Password save policy (`GPasswordSave`).
#[repr(u32)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PasswordSave {
    Never = 0,
    ForSession = 1,
    Permanently = 2,
}

/// Mount operation result (`GMountOperationResult`).
#[repr(u32)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum MountOperationResult {
    Handled = 0,
    Aborted = 1,
    Unhandled = 2,
}

/// Ask password flags (`GAskPasswordFlags`).
#[repr(u32)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum AskPasswordFlags {
    NeedPassword = 1 << 0,
    NeedUsername = 1 << 1,
    NeedDomain = 1 << 2,
    SavingSupported = 1 << 3,
    AnonymousSupported = 1 << 4,
    Tcrypt = 1 << 5,
}

/// A mount operation (`GMountOperation`).
pub struct MountOperation {
    username: Mutex<Option<String>>,
    password: Mutex<Option<String>>,
    anonymous: Mutex<bool>,
    domain: Mutex<Option<String>>,
    password_save: Mutex<PasswordSave>,
    choice: Mutex<i32>,
    is_tcrypt_hidden_volume: Mutex<bool>,
    is_tcrypt_system_volume: Mutex<bool>,
    pim: Mutex<u32>,
}

impl MountOperation {
    /// Creates a new mount operation.
    ///
    /// Mirrors `g_mount_operation_new`.
    pub fn new() -> Self {
        Self {
            username: Mutex::new(None),
            password: Mutex::new(None),
            anonymous: Mutex::new(false),
            domain: Mutex::new(None),
            password_save: Mutex::new(PasswordSave::Never),
            choice: Mutex::new(0),
            is_tcrypt_hidden_volume: Mutex::new(false),
            is_tcrypt_system_volume: Mutex::new(false),
            pim: Mutex::new(0),
        }
    }

    pub fn get_username(&self) -> Option<String> {
        self.username.lock().clone()
    }

    pub fn set_username(&self, username: Option<&str>) {
        *self.username.lock() = username.map(|s| s.to_string());
    }

    pub fn get_password(&self) -> Option<String> {
        self.password.lock().clone()
    }

    pub fn set_password(&self, password: Option<&str>) {
        *self.password.lock() = password.map(|s| s.to_string());
    }

    pub fn get_anonymous(&self) -> bool {
        *self.anonymous.lock()
    }

    pub fn set_anonymous(&self, anonymous: bool) {
        *self.anonymous.lock() = anonymous;
    }

    pub fn get_domain(&self) -> Option<String> {
        self.domain.lock().clone()
    }

    pub fn set_domain(&self, domain: Option<&str>) {
        *self.domain.lock() = domain.map(|s| s.to_string());
    }

    pub fn get_password_save(&self) -> PasswordSave {
        *self.password_save.lock()
    }

    pub fn set_password_save(&self, save: PasswordSave) {
        *self.password_save.lock() = save;
    }

    pub fn get_choice(&self) -> i32 {
        *self.choice.lock()
    }

    pub fn set_choice(&self, choice: i32) {
        *self.choice.lock() = choice;
    }

    pub fn reply(&self, _result: MountOperationResult) {
        // In upstream this emits the "reply" signal
    }

    pub fn get_is_tcrypt_hidden_volume(&self) -> bool {
        *self.is_tcrypt_hidden_volume.lock()
    }

    pub fn set_is_tcrypt_hidden_volume(&self, hidden_volume: bool) {
        *self.is_tcrypt_hidden_volume.lock() = hidden_volume;
    }

    pub fn get_is_tcrypt_system_volume(&self) -> bool {
        *self.is_tcrypt_system_volume.lock()
    }

    pub fn set_is_tcrypt_system_volume(&self, system_volume: bool) {
        *self.is_tcrypt_system_volume.lock() = system_volume;
    }

    pub fn get_pim(&self) -> u32 {
        *self.pim.lock()
    }

    pub fn set_pim(&self, pim: u32) {
        *self.pim.lock() = pim;
    }
}

impl Default for MountOperation {
    fn default() -> Self {
        Self::new()
    }
}

// ──────────────────────────── Tests ───────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mount_operation_new() {
        let op = MountOperation::new();
        assert!(op.get_username().is_none());
        assert!(op.get_password().is_none());
        assert!(!op.get_anonymous());
        assert!(op.get_domain().is_none());
        assert_eq!(op.get_password_save(), PasswordSave::Never);
        assert_eq!(op.get_choice(), 0);
    }

    #[test]
    fn test_set_get_username() {
        let op = MountOperation::new();
        op.set_username(Some("admin"));
        assert_eq!(op.get_username().unwrap(), "admin");
        op.set_username(None);
        assert!(op.get_username().is_none());
    }

    #[test]
    fn test_set_get_password() {
        let op = MountOperation::new();
        op.set_password(Some("secret"));
        assert_eq!(op.get_password().unwrap(), "secret");
    }

    #[test]
    fn test_set_get_anonymous() {
        let op = MountOperation::new();
        op.set_anonymous(true);
        assert!(op.get_anonymous());
    }

    #[test]
    fn test_set_get_domain() {
        let op = MountOperation::new();
        op.set_domain(Some("WORKGROUP"));
        assert_eq!(op.get_domain().unwrap(), "WORKGROUP");
    }

    #[test]
    fn test_set_get_password_save() {
        let op = MountOperation::new();
        op.set_password_save(PasswordSave::Permanently);
        assert_eq!(op.get_password_save(), PasswordSave::Permanently);
    }

    #[test]
    fn test_set_get_choice() {
        let op = MountOperation::new();
        op.set_choice(2);
        assert_eq!(op.get_choice(), 2);
    }

    #[test]
    fn test_tcrypt_params() {
        let op = MountOperation::new();
        op.set_is_tcrypt_hidden_volume(true);
        assert!(op.get_is_tcrypt_hidden_volume());
        op.set_is_tcrypt_system_volume(true);
        assert!(op.get_is_tcrypt_system_volume());
        op.set_pim(500);
        assert_eq!(op.get_pim(), 500);
    }

    #[test]
    fn test_reply() {
        let op = MountOperation::new();
        op.reply(MountOperationResult::Handled);
    }

    #[test]
    fn test_enum_values() {
        assert_eq!(PasswordSave::Never as u32, 0);
        assert_eq!(PasswordSave::ForSession as u32, 1);
        assert_eq!(PasswordSave::Permanently as u32, 2);
        assert_eq!(MountOperationResult::Handled as u32, 0);
        assert_eq!(MountOperationResult::Aborted as u32, 1);
        assert_eq!(MountOperationResult::Unhandled as u32, 2);
    }
}
