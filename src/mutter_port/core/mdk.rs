//! Development kit (mdk) ported from GNOME Mutter's src/core/meta-mdk.c
//!
//! `MetaMdk` launches the external mutter-devkit viewer subprocess and exports
//! the `org.gnome.Mutter.Devkit` D-Bus interface so the viewer can obtain the
//! Wayland/X11 display environment of the running compositor.
//!
//! Only built upstream when `HAVE_DEVKIT`. Subprocess spawning and D-Bus are
//! unavailable in the kernel, so the launch/export paths are stubbed while the
//! state machine and environment plumbing are kept faithful.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/core/meta-mdk.c

use alloc::string::{String, ToString};

/// Path to the external devkit viewer binary (MUTTER_LIBEXECDIR "/mutter-devkit").
const DEVKIT_PATH: &str = "/usr/libexec/mutter-devkit";

/// D-Bus object path for the devkit interface.
const DEVKIT_DBUS_PATH: &str = "/org/gnome/Mutter/Devkit";

/// D-Bus well-known name owned by the devkit.
const DEVKIT_DBUS_NAME: &str = "org.gnome.Mutter.Devkit";

/// Behavior flags, mirrors MetaMdkFlag.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MdkFlag {
    /// META_MDK_FLAG_NONE
    None,
    /// META_MDK_FLAG_LAUNCH_VIEWER
    LaunchViewer,
}

/// Development kit instance, mirrors MetaMdk.
#[derive(Debug)]
pub struct Mdk {
    flags: MdkFlag,
    /// The compositor's own WAYLAND_DISPLAY, inherited from the environment.
    external_wayland_display: Option<String>,
    /// The compositor's own DISPLAY, inherited from the environment.
    external_x11_display: Option<String>,
    devkit_args: Option<String>,
    dbus_name_owned: bool,
    process_running: bool,
}

impl Mdk {
    /// meta_mdk_new(): capture the external display environment and, when
    /// requested, launch the viewer and own the D-Bus name.
    pub fn new(
        flags: MdkFlag,
        args: Option<String>,
        external_wayland_display: Option<String>,
        external_x11_display: Option<String>,
    ) -> Result<Self, &'static str> {
        let mut mdk = Mdk {
            flags,
            external_wayland_display,
            external_x11_display,
            devkit_args: args,
            dbus_name_owned: false,
            process_running: false,
        };

        if flags == MdkFlag::LaunchViewer {
            mdk.launch()?;
        }

        // g_bus_own_name (G_BUS_TYPE_SESSION, "org.gnome.Mutter.Devkit", ...)
        mdk.dbus_name_owned = true;

        Ok(mdk)
    }

    /// launch_devkit(): spawn the viewer subprocess with the compositor's
    /// display environment. Requires screen-cast/remote-desktop to be enabled;
    /// stubbed here since there is no subprocess support in the kernel.
    fn launch(&mut self) -> Result<(), &'static str> {
        let _path = DEVKIT_PATH;
        let _args = self.devkit_args.as_deref();
        // Build environment: WAYLAND_DISPLAY / DISPLAY are set from the
        // captured external displays, or unset if absent.
        self.process_running = true;
        Ok(())
    }

    /// on_devkit_process_died() equivalent — mark the process as gone.
    pub fn on_process_died(&mut self) {
        self.process_running = false;
    }

    /// Whether the D-Bus interface is exported at DEVKIT_DBUS_PATH.
    pub fn is_exported(&self) -> bool {
        let _path = DEVKIT_DBUS_PATH;
        let _name = DEVKIT_DBUS_NAME;
        self.dbus_name_owned
    }

    pub fn is_running(&self) -> bool {
        self.process_running
    }

    pub fn flags(&self) -> MdkFlag {
        self.flags
    }

    /// Environment the viewer inherits (WAYLAND_DISPLAY, DISPLAY).
    pub fn viewer_environment(&self) -> Environment {
        Environment {
            wayland_display: self.external_wayland_display.clone(),
            x11_display: self.external_x11_display.clone(),
            xauthority: None,
        }
    }
}

impl Drop for Mdk {
    fn drop(&mut self) {
        // g_bus_unown_name + cancel/kill subprocess.
        self.dbus_name_owned = false;
        self.process_running = false;
    }
}

/// Environment exported over the devkit D-Bus GetEnvironment call.
#[derive(Debug, Clone, Default)]
pub struct Environment {
    pub wayland_display: Option<String>,
    pub x11_display: Option<String>,
    pub xauthority: Option<String>,
}

impl Environment {
    /// on_handle_get_environment(): fill the variant with the running
    /// compositor's Wayland display name and, if X11 is enabled, DISPLAY and
    /// XAUTHORITY.
    pub fn to_pairs(&self) -> alloc::vec::Vec<(String, String)> {
        let mut out = alloc::vec::Vec::new();
        if let Some(w) = &self.wayland_display {
            out.push(("WAYLAND_DISPLAY".to_string(), w.clone()));
        }
        if let Some(x) = &self.x11_display {
            out.push(("DISPLAY".to_string(), x.clone()));
        }
        if let Some(a) = &self.xauthority {
            out.push(("XAUTHORITY".to_string(), a.clone()));
        }
        out
    }
}
