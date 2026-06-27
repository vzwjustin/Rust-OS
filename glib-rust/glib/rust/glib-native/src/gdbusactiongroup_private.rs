//! `gdbusactiongroup-private` matching `gio/gdbusactiongroup-private.h`.
//!
//! Private DBus action group API: sync action group state.
//!
//! Fully `no_std` compatible.

use crate::gdbusactiongroup::DBusActionGroup;
use alloc::string::String;

/// Syncs the action group state from the DBus connection
/// (mirrors `g_dbus_action_group_sync`).
///
/// In our no_std port, this is a no-op since we don't have a real DBus connection.
pub fn sync(_group: &DBusActionGroup) -> Result<(), String> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sync_noop() {
        let group = DBusActionGroup::new("org.test", "/org/test");
        assert!(sync(&group).is_ok());
    }
}
