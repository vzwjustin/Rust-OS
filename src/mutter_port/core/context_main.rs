//! Main context ported from GNOME Mutter's src/core/meta-context-main.c
//!
//! The concrete `MetaContext` subclass used when running mutter as a real
//! display server (native/headless). Handles command-line options, backend
//! selection, persistent virtual monitors, session manager creation, and
//! (optionally) the development kit (mdk).
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/core/meta-context-main.c

use alloc::string::{String, ToString};
use alloc::vec::Vec;

use super::session_manager::SessionManager;

impl ContextMainOptions {
    /// Parse a Mutter `meta-context-main` style argv slice into RustOS options.
    ///
    /// This intentionally accepts the mutter-main flags RustOS can currently
    /// wire into real state. Unsupported flags are rejected instead of being
    /// silently treated as no-ops.
    pub fn parse_args<'a, I>(&mut self, args: I) -> Result<(), &'static str>
    where
        I: IntoIterator<Item = &'a str>,
    {
        let mut iter = args.into_iter();

        while let Some(arg) = iter.next() {
            match arg {
                "--wayland" => self.wayland = true,
                "--no-x11" => self.no_x11 = true,
                "--display-server" => self.display_server = true,
                "--headless" => self.headless = true,
                "--devkit" => self.devkit = true,
                "--debug-control" => self.debug_control = true,
                "--unsafe-mode" => self.unsafe_mode = true,
                "--wayland-display" => {
                    let value = iter.next().ok_or("missing --wayland-display value")?;
                    self.wayland_display = Some(value.to_string());
                }
                "--devkit-args" => {
                    let value = iter.next().ok_or("missing --devkit-args value")?;
                    self.devkit_args = Some(value.to_string());
                }
                "--virtual-monitor" => {
                    let value = iter.next().ok_or("missing --virtual-monitor value")?;
                    self.add_virtual_monitor_spec(value)?;
                }
                "--trace-file" => {
                    let value = iter.next().ok_or("missing --trace-file value")?;
                    self.trace_file = Some(value.to_string());
                }
                value if value.starts_with("--wayland-display=") => {
                    self.wayland_display = Some(value["--wayland-display=".len()..].to_string());
                }
                value if value.starts_with("--devkit-args=") => {
                    self.devkit_args = Some(value["--devkit-args=".len()..].to_string());
                }
                value if value.starts_with("--virtual-monitor=") => {
                    self.add_virtual_monitor_spec(&value["--virtual-monitor=".len()..])?;
                }
                value if value.starts_with("--trace-file=") => {
                    self.trace_file = Some(value["--trace-file=".len()..].to_string());
                }
                _ => return Err("unsupported mutter-main argument"),
            }
        }

        Ok(())
    }

    fn add_virtual_monitor_spec(&mut self, spec: &str) -> Result<(), &'static str> {
        let (width, height, refresh_rate) = parse_monitor_mode(spec)?;
        let serial = virtual_monitor_serial(self.virtual_monitor_infos.len());

        self.virtual_monitor_infos
            .push(VirtualMonitorInfo::new_simple(
                width,
                height,
                refresh_rate,
                "MetaVendor",
                "MetaVirtualMonitor",
                serial,
            ));

        Ok(())
    }
}

fn virtual_monitor_serial(index: usize) -> String {
    let mut serial = String::from("0x");
    let hi = (index >> 4) & 0xf;
    let lo = index & 0xf;
    serial.push(hex_digit(hi as u8));
    serial.push(hex_digit(lo as u8));
    serial
}

impl ContextMain {
    /// Build a main context and apply supported mutter-main argv flags.
    pub fn with_args<'a, I>(name: &str, args: I) -> Result<Self, &'static str>
    where
        I: IntoIterator<Item = &'a str>,
    {
        let mut context = Self::new(name);
        context.options.parse_args(args)?;
        Ok(context)
    }

    /// Parsed mutter-main options.
    pub fn options(&self) -> &ContextMainOptions {
        &self.options
    }

    /// Mutable parsed mutter-main options.
    pub fn options_mut(&mut self) -> &mut ContextMainOptions {
        &mut self.options
    }

    /// Human-readable Mutter context name.
    pub fn name(&self) -> &str {
        &self.nick
    }

    /// Number of virtual monitors promoted into persistent configuration.
    pub fn persistent_virtual_monitor_count(&self) -> usize {
        self.persistent_virtual_monitors
    }
}

/// X11 display policy, mirrors MetaX11DisplayPolicy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum X11DisplayPolicy {
    Disabled,
    OnDemand,
    Mandatory,
}

/// Native backend mode used when creating the backend.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackendMode {
    Default,
    Headless,
}

/// Description of a virtual monitor requested on the command line.
#[derive(Debug, Clone)]
pub struct VirtualMonitorInfo {
    pub width: i32,
    pub height: i32,
    pub refresh_rate: f32,
    pub vendor: String,
    pub product: String,
    pub serial: String,
}

impl VirtualMonitorInfo {
    pub fn new_simple(
        width: i32,
        height: i32,
        refresh_rate: f32,
        vendor: &str,
        product: &str,
        serial: String,
    ) -> Self {
        VirtualMonitorInfo {
            width,
            height,
            refresh_rate,
            vendor: vendor.to_string(),
            product: product.to_string(),
            serial,
        }
    }
}

/// Command-line derived options, mirrors MetaContextMainOptions.
#[derive(Debug, Default)]
pub struct ContextMainOptions {
    pub wayland: bool,
    pub no_x11: bool,
    pub wayland_display: Option<String>,
    pub display_server: bool,
    pub headless: bool,
    pub devkit: bool,
    pub devkit_args: Option<String>,
    pub virtual_monitor_infos: Vec<VirtualMonitorInfo>,
    pub trace_file: Option<String>,
    pub debug_control: bool,
    pub unsafe_mode: bool,
}

/// Concrete main context, mirrors MetaContextMain.
#[derive(Debug)]
pub struct ContextMain {
    pub options: ContextMainOptions,
    session_manager: Option<SessionManager>,
    persistent_virtual_monitors: usize,
    nick: String,
}

impl ContextMain {
    /// Corresponds to meta_create_context().
    pub fn new(name: &str) -> Self {
        ContextMain {
            options: ContextMainOptions::default(),
            session_manager: None,
            persistent_virtual_monitors: 0,
            nick: name.to_string(),
        }
    }

    /// meta_context_main_configure(): validate options and normalize env.
    pub fn configure(&mut self) -> Result<(), &'static str> {
        // wayland_display is consumed (stolen) here.
        let _wayland_display = self.options.wayland_display.take();
        self.check_configuration()?;
        // g_unsetenv ("DESKTOP_AUTOSTART_ID") — no-op in kernel context.
        Ok(())
    }

    /// check_configuration(): display-server mode is incompatible with
    /// headless / devkit operation.
    fn check_configuration(&self) -> Result<(), &'static str> {
        if self.options.display_server && (self.options.headless || self.options.devkit) {
            return Err("Can't run in display server mode headlessly");
        }
        Ok(())
    }

    /// meta_context_main_get_x11_display_policy().
    pub fn x11_display_policy(&self) -> X11DisplayPolicy {
        if self.options.no_x11 {
            return X11DisplayPolicy::Disabled;
        }
        // Without a systemd user unit we treat X11 as mandatory; otherwise
        // on-demand. In the kernel there is no logind, so default to mandatory.
        X11DisplayPolicy::Mandatory
    }

    /// add_persistent_virtual_monitors(): would create the monitors on the
    /// backend's monitor manager. Here we only track the count.
    pub fn add_persistent_virtual_monitors(&mut self) -> Result<(), &'static str> {
        let count = self.options.virtual_monitor_infos.len();
        self.persistent_virtual_monitors += count;
        self.options.virtual_monitor_infos.clear();
        Ok(())
    }

    /// meta_context_main_setup().
    pub fn setup(&mut self) -> Result<(), &'static str> {
        self.add_persistent_virtual_monitors()?;
        if !self.options.devkit && self.options.devkit_args.is_some() {
            self.options.devkit_args = None;
            // g_warning ("Passed --devkit-args but not --devkit");
        }
        Ok(())
    }

    /// meta_context_main_create_backend(): choose native or headless backend.
    pub fn create_backend_mode(&self) -> BackendMode {
        if self.options.headless || self.options.devkit {
            BackendMode::Headless
        } else {
            BackendMode::Default
        }
    }

    /// meta_context_main_notify_ready(): create the session manager.
    pub fn notify_ready(&mut self) {
        self.session_manager = Some(SessionManager::new(self.nick.clone()));
    }

    /// meta_context_main_get_session_manager().
    pub fn session_manager(&self) -> Option<&SessionManager> {
        self.session_manager.as_ref()
    }

    /// add_virtual_monitor_cb(): parse a "WxH" / "WxH@R" spec and register it.
    pub fn add_virtual_monitor(&mut self, spec: &str) -> Result<(), &'static str> {
        let (width, height, refresh_rate) = parse_monitor_mode(spec)?;
        let serial = {
            let idx = self.options.virtual_monitor_infos.len();
            let mut s = String::from("0x");
            let hi = (idx >> 4) & 0xf;
            let lo = idx & 0xf;
            s.push(hex_digit(hi as u8));
            s.push(hex_digit(lo as u8));
            s
        };
        self.options
            .virtual_monitor_infos
            .push(VirtualMonitorInfo::new_simple(
                width,
                height,
                refresh_rate,
                "MetaVendor",
                "MetaVirtualMonitor",
                serial,
            ));
        Ok(())
    }
}

fn hex_digit(v: u8) -> char {
    match v {
        0..=9 => (b'0' + v) as char,
        _ => (b'a' + (v - 10)) as char,
    }
}

/// meta_parse_monitor_mode(): parse "WxH" or "WxH@R".
fn parse_monitor_mode(spec: &str) -> Result<(i32, i32, f32), &'static str> {
    let (dims, rate) = match spec.split_once('@') {
        Some((d, r)) => (d, r.parse::<f32>().unwrap_or(60.0)),
        None => (spec, 60.0),
    };
    let (w, h) = dims
        .split_once('x')
        .ok_or("Unrecognizable virtual monitor spec")?;
    let width = w.trim().parse::<i32>().map_err(|_| "invalid width")?;
    let height = h.trim().parse::<i32>().map_err(|_| "invalid height")?;
    Ok((width, height, rate))
}
