//! Kernel command line parsing
//!
//! Mirrors the subset of Linux's `init/do_mounts.c` / `init/main.c` command
//! line handling that this kernel needs at boot:
//!
//!   - `root=<dev>`       -> `root_dev_setup()` in do_mounts.c
//!   - `rootfstype=<fs>`  -> `fs_names` handling in do_mounts.c (`mount_root`)
//!   - `init=<path>`      -> `execute_command` in init/main.c, consumed by
//!                           `kernel_init` / `run_init_process`
//!
//! ## Why this exists separately from `initramfs.rs`
//!
//! Today the kernel has no real source for a boot command line: the `bootloader`
//! 0.9.x crate's `BootInfo` does not carry one, and the multiboot entry point
//! (`src/boot.s` / `rust_main` in `src/main.rs`) is an unimplemented stub. So
//! `parse_cmdline` below is pure, allocation-light string parsing that can be
//! unit tested and wired up the moment a real cmdline source (Limine boot
//! protocol, multiboot2 `OS_CMDLINE` tag, etc.) lands. Until then,
//! `boot_cmdline()` returns the compiled-in `DEFAULT_CMDLINE` (empty),
//! matching Linux's behavior when no `root=`/`init=` arguments are given:
//! the kernel falls back to its built-in defaults (here, the
//! `/sbin/init`, `/etc/init`, `/bin/init`, `/bin/sh` search order in
//! `initramfs.rs`).
//!
//! ## Root device / fstype
//!
//! RustOS currently boots from a single embedded cpio initramfs that is
//! extracted directly onto the in-memory root VFS (see `initramfs.rs`); there
//! is no block-device root to switch to, so `root=`/`rootfstype=` are parsed
//! for fidelity and future use (e.g. mounting a real disk as `/`) but are not
//! yet consulted by the mount path. `RootParam::Unspecified` is the only
//! value produced until that lands.

use alloc::string::{String, ToString};

/// Default compiled-in command line. Empty: no real cmdline source is wired
/// up yet (see module docs). Override at boot via `set_boot_cmdline` once a
/// loader-provided cmdline is available.
pub const DEFAULT_CMDLINE: &str = "";

/// `root=` value, matching Linux's `ROOT_DEV` conventions just enough to be
/// parsed; not yet consulted for mounting (see module docs).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RootParam {
    /// No `root=` argument was present.
    Unspecified,
    /// `root=/dev/<name>` or `root=PARTUUID=...` / `root=UUID=...` — stored
    /// verbatim, as Linux also defers resolution to `name_to_dev_t()`.
    Device(String),
}

impl Default for RootParam {
    fn default() -> Self {
        RootParam::Unspecified
    }
}

/// Parsed kernel command line, holding only the subset this kernel acts on.
#[derive(Debug, Clone, Default)]
pub struct KernelCmdline {
    /// `root=...`
    pub root: RootParam,
    /// `rootfstype=...`
    pub rootfstype: Option<String>,
    /// `init=...` — Linux: overrides the default init search order
    /// (`/sbin/init`, `/etc/init`, `/bin/init`, `/bin/sh`) and, unlike that
    /// fallback chain, a failed exec of an explicit `init=` is fatal in
    /// Linux. See `find_userspace_init_path` in `initramfs.rs` for how this
    /// is consulted.
    pub init: Option<String>,
}

/// Parse a Linux-style kernel command line into the parameters this kernel
/// understands. Unknown `key=value` and bare tokens are ignored, matching
/// `parse_args()`'s tolerance of unrecognized parameters in Linux.
///
/// Quoting (`foo="bar baz"`) is not implemented; none of `root=`,
/// `rootfstype=`, or `init=` need it for the paths this kernel supports.
pub fn parse_cmdline(cmdline: &str) -> KernelCmdline {
    let mut parsed = KernelCmdline::default();

    for token in cmdline.split_whitespace() {
        let Some((key, value)) = token.split_once('=') else {
            continue;
        };
        if value.is_empty() {
            continue;
        }
        match key {
            "root" => parsed.root = RootParam::Device(value.to_string()),
            "rootfstype" => parsed.rootfstype = Some(value.to_string()),
            "init" => parsed.init = Some(value.to_string()),
            _ => {}
        }
    }

    parsed
}

use spin::RwLock;

static BOOT_CMDLINE: RwLock<Option<KernelCmdline>> = RwLock::new(None);

/// Record the boot command line once a real source is available (loader
/// cmdline, etc.). Safe to call multiple times; last write wins.
pub fn set_boot_cmdline(raw: &str) {
    let parsed = parse_cmdline(raw);
    *BOOT_CMDLINE.write() = Some(parsed);
}

/// Current parsed boot command line, falling back to `DEFAULT_CMDLINE`
/// (currently empty -- see module docs) if `set_boot_cmdline` was never
/// called.
pub fn boot_cmdline() -> KernelCmdline {
    BOOT_CMDLINE
        .read()
        .clone()
        .unwrap_or_else(|| parse_cmdline(DEFAULT_CMDLINE))
}

/// `init=` override, if the boot command line specified one. Consulted by
/// `initramfs::find_userspace_init` before falling back to the Linux-style
/// search order.
pub fn init_override() -> Option<String> {
    boot_cmdline().init
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_root_rootfstype_init() {
        let c = parse_cmdline("console=ttyS0 root=/dev/sda1 rootfstype=ext4 init=/bin/myinit quiet");
        assert_eq!(c.root, RootParam::Device("/dev/sda1".to_string()));
        assert_eq!(c.rootfstype.as_deref(), Some("ext4"));
        assert_eq!(c.init.as_deref(), Some("/bin/myinit"));
    }

    #[test]
    fn empty_cmdline_is_all_unspecified() {
        let c = parse_cmdline("");
        assert_eq!(c.root, RootParam::Unspecified);
        assert!(c.rootfstype.is_none());
        assert!(c.init.is_none());
    }

    #[test]
    fn ignores_unknown_and_bare_tokens() {
        let c = parse_cmdline("quiet splash foo=bar root=PARTUUID=1234");
        assert_eq!(c.root, RootParam::Device("PARTUUID=1234".to_string()));
        assert!(c.init.is_none());
    }
}
