//! `gunixmounts-private` matching `gio/gunixmounts-private.h`.
//!
//! System mount paths that should be hidden from users.
//!
//! Fully `no_std` compatible.

/// System mount paths to hide from the user (mirrors `system_mount_paths[]`).
///
/// Includes all FHS 2.3 toplevel dirs and other specialized directories.
pub static SYSTEM_MOUNT_PATHS: &[&str] = &[
    "/", // we already have "Filesystem root" in Nautilus
    "/bin",
    "/boot",
    "/compat/linux/proc",
    "/compat/linux/sys",
    "/dev",
    "/etc",
    "/home",
    "/lib",
    "/lib64",
    "/libexec",
    "/live/cow",
    "/live/image",
    "/media",
    "/mnt",
    "/net",
    "/opt",
    "/proc",
    "/rescue",
    "/root",
    "/sbin",
    "/srv",
    "/sys",
    "/tmp",
    "/usr",
    "/usr/X11R6",
    "/usr/local",
    "/usr/obj",
    "/usr/ports",
    "/usr/src",
    "/usr/xobj",
    "/var",
    "/var/crash",
    "/var/local",
    "/var/log",
    "/var/log/audit",
    "/var/mail",
    "/var/run",
    "/var/tmp",
];

/// Checks if a mount path is a system path that should be hidden.
pub fn is_system_mount_path(path: &str) -> bool {
    SYSTEM_MOUNT_PATHS.binary_search(&path).is_ok()
}

/// Returns all system mount paths.
pub fn get_system_mount_paths() -> &'static [&'static str] {
    SYSTEM_MOUNT_PATHS
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_system_mount_path() {
        assert!(is_system_mount_path("/"));
        assert!(is_system_mount_path("/bin"));
        assert!(is_system_mount_path("/usr"));
        assert!(is_system_mount_path("/var/tmp"));
        assert!(!is_system_mount_path("/home/user"));
        assert!(!is_system_mount_path("/media/usb"));
    }

    #[test]
    fn test_get_system_mount_paths() {
        let paths = get_system_mount_paths();
        assert!(!paths.is_empty());
        assert!(paths.contains(&"/"));
        assert!(paths.contains(&"/usr"));
    }

    #[test]
    fn test_sorted() {
        // Verify the array is sorted for binary search
        let paths = get_system_mount_paths();
        for i in 1..paths.len() {
            assert!(
                paths[i - 1] <= paths[i],
                "unsorted at index {}: {} > {}",
                i,
                paths[i - 1],
                paths[i]
            );
        }
    }
}
