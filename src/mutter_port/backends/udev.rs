//! GNOME Mutter's src/backends/meta-udev.c
//!
//! udev integration: enumerates DRM (GPU) and backlight devices, classifies
//! them (boot VGA, platform device, seat membership, mutter device tags), and
//! turns udev "uevent"s into add/remove/hotplug/lease/backlight signals.
//!
//! Stubbed: GUdevClient, the udev netlink monitor, GUdevEnumerator, and GObject
//! signals do not exist in the kernel. A GUdevDevice is modeled by `UdevDevice`
//! — a plain snapshot of the properties, sysfs attributes, tags and parent
//! chain that the C queries. Device classification and uevent dispatch logic
//! are ported faithfully against that model.
//!
//! Reference:
//! https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/backends/meta-udev.c

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;

const DRM_CARD_UDEV_DEVICE_TYPE: &str = "drm_minor";

/// MetaUdevDeviceType.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UdevDeviceType {
    Card,
    RenderNode,
}

/// GUdevDeviceType (device node kind).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeviceNodeType {
    None,
    Char,
    Block,
}

/// Signals emitted from uevent handling, replacing the GObject signals.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UdevSignal {
    Hotplug,
    Lease,
    DeviceAdded,
    DeviceRemoved,
    BacklightChanged,
}

/// A snapshot of a udev device (GUdevDevice). Holds the fields the port needs:
/// name, subsystem, node type, device file, properties, sysfs attributes,
/// tags, and a parent chain keyed by subsystem.
#[derive(Debug, Clone, Default)]
pub struct UdevDevice {
    pub name: String,
    pub subsystem: String,
    pub node_type: DeviceNodeType,
    /// The device file (g_udev_device_get_device_file); None if absent.
    pub device_file: Option<String>,
    pub properties: BTreeMap<String, String>,
    pub sysfs_attrs: BTreeMap<String, String>,
    pub tags: Vec<String>,
    pub current_tags: Vec<String>,
    /// Parent devices, keyed by their subsystem (get_parent_with_subsystem).
    /// A single "devtype" match is folded into the key as "subsystem/devtype".
    pub parents: BTreeMap<String, alloc::boxed::Box<UdevDevice>>,
}

impl Default for DeviceNodeType {
    fn default() -> Self {
        DeviceNodeType::None
    }
}

impl UdevDevice {
    /// g_udev_device_get_property()
    pub fn property(&self, key: &str) -> Option<&str> {
        self.properties.get(key).map(|s| s.as_str())
    }

    /// g_udev_device_get_property_as_boolean()
    pub fn property_as_bool(&self, key: &str) -> bool {
        matches!(self.property(key), Some("1") | Some("true"))
    }

    /// g_udev_device_get_sysfs_attr()
    pub fn sysfs_attr(&self, key: &str) -> Option<&str> {
        self.sysfs_attrs.get(key).map(|s| s.as_str())
    }

    /// g_udev_device_get_sysfs_attr_as_int()
    pub fn sysfs_attr_as_int(&self, key: &str) -> i64 {
        self.sysfs_attr(key)
            .and_then(|v| v.parse::<i64>().ok())
            .unwrap_or(0)
    }

    /// g_udev_device_get_parent_with_subsystem() (subsystem only).
    pub fn parent_with_subsystem(&self, subsystem: &str) -> Option<&UdevDevice> {
        self.parents.get(subsystem).map(|b| b.as_ref())
    }

    /// g_udev_device_get_parent_with_subsystem() (subsystem + devtype).
    pub fn parent_with_subsystem_devtype(
        &self,
        subsystem: &str,
        devtype: &str,
    ) -> Option<&UdevDevice> {
        let mut key = String::from(subsystem);
        key.push('/');
        key.push_str(devtype);
        self.parents.get(&key).map(|b| b.as_ref())
    }
}

/// meta_is_udev_device_platform_device()
pub fn is_udev_device_platform_device(device: &UdevDevice) -> bool {
    device.parent_with_subsystem("platform").is_some()
}

/// meta_is_udev_device_boot_vga()
pub fn is_udev_device_boot_vga(device: &UdevDevice) -> bool {
    if let Some(pci) = device.parent_with_subsystem("pci") {
        if pci.sysfs_attr_as_int("boot_vga") == 1 {
            return true;
        }
    }
    if let Some(drm) = device.parent_with_subsystem("drm") {
        if drm.sysfs_attr_as_int("boot_display") == 1 {
            return true;
        }
    }
    false
}

/// meta_has_udev_device_tag(): checks the device's own tags, then recurses into
/// the platform-parent chain.
fn has_udev_device_tag(device: &UdevDevice, tag: &str) -> bool {
    if device.tags.iter().any(|t| t == tag) {
        return true;
    }
    match device.parent_with_subsystem("platform") {
        Some(platform) => has_udev_device_tag(platform, tag),
        None => false,
    }
}

/// meta_is_udev_device_disable_modifiers()
pub fn is_udev_device_disable_modifiers(device: &UdevDevice) -> bool {
    has_udev_device_tag(device, "mutter-device-disable-kms-modifiers")
}

/// meta_is_udev_device_ignore()
pub fn is_udev_device_ignore(device: &UdevDevice) -> bool {
    has_udev_device_tag(device, "mutter-device-ignore")
}

/// meta_is_udev_test_device()
pub fn is_udev_test_device(device: &UdevDevice) -> bool {
    if let Some(devpath) = device.property("DEVPATH") {
        if devpath.starts_with("/devices/faux/vkms/drm/card") {
            return true;
        }
    }
    device.property("ID_PATH") == Some("platform-vkms")
}

/// meta_is_udev_device_preferred_primary()
pub fn is_udev_device_preferred_primary(device: &UdevDevice) -> bool {
    device
        .current_tags
        .iter()
        .any(|t| t == "mutter-device-preferred-primary")
}

/// MetaUdev. Mirrors struct _MetaUdev (minus the GUdevClient handle).
#[derive(Debug)]
pub struct Udev {
    /// Whether the (stubbed) uevent handler is currently active.
    handler_active: bool,
    /// Whether the backend is headless (meta_backend_is_headless stub).
    headless: bool,
    /// This seat's id (meta_launcher_get_seat_id stub); None => "seat0".
    seat_id: Option<String>,
}

impl Default for Udev {
    fn default() -> Self {
        Self::new()
    }
}

impl Udev {
    /// meta_udev_new() / meta_udev_init(): would create a GUdevClient for the
    /// "drm" and "backlight" subsystems and connect the uevent handler.
    pub fn new() -> Self {
        Udev {
            handler_active: true,
            headless: false,
            seat_id: None,
        }
    }

    pub fn set_headless(&mut self, headless: bool) {
        self.headless = headless;
    }

    pub fn set_seat_id(&mut self, seat_id: Option<String>) {
        self.seat_id = seat_id;
    }

    /// meta_udev_is_drm_device()
    pub fn is_drm_device(&self, device: &UdevDevice) -> bool {
        // Filter out non-character devices (e.g. card0-VGA-1).
        if device.node_type != DeviceNodeType::Char {
            return false;
        }
        if device.property("DEVTYPE") != Some(DRM_CARD_UDEV_DEVICE_TYPE) {
            return false;
        }

        // Skip devices that do not belong to our seat.
        if !self.headless {
            let device_seat = device.property("ID_SEAT").unwrap_or("seat0");
            let seat_id = self.seat_id.as_deref();
            // g_strcmp0(seat_id, device_seat) != 0  => reject.
            if seat_id != Some(device_seat) {
                return false;
            }
        }
        true
    }

    /// meta_udev_list_drm_devices(): filter a set of enumerated devices to the
    /// DRM devices belonging to this seat. (The GUdevEnumerator that matches
    /// "card*"/"render*" + subsystem "drm" is stubbed — the caller supplies the
    /// candidate list.)
    pub fn list_drm_devices(
        &self,
        _device_type: UdevDeviceType,
        candidates: Vec<UdevDevice>,
    ) -> Vec<UdevDevice> {
        candidates
            .into_iter()
            .filter(|d| self.is_drm_device(d))
            .collect()
    }

    /// on_uevent()/on_drm_uevent()/on_backlight_uevent(): translate a uevent
    /// into the signals it would emit. Returns an empty list when paused.
    pub fn handle_uevent(&self, action: &str, device: &UdevDevice) -> Vec<UdevSignal> {
        let mut signals = Vec::new();
        if !self.handler_active {
            return signals;
        }

        match device.subsystem.as_str() {
            "drm" => {
                // Ignore devices without a device file.
                if device.device_file.is_none() {
                    return signals;
                }
                match action {
                    "add" => signals.push(UdevSignal::DeviceAdded),
                    "remove" => signals.push(UdevSignal::DeviceRemoved),
                    _ => {}
                }
                if device.property_as_bool("HOTPLUG") {
                    signals.push(UdevSignal::Hotplug);
                }
                if device.property_as_bool("LEASE") {
                    signals.push(UdevSignal::Lease);
                }
            }
            "backlight" => {
                if action == "change" {
                    signals.push(UdevSignal::BacklightChanged);
                }
            }
            _ => {}
        }
        signals
    }

    /// meta_udev_pause()
    pub fn pause(&mut self) {
        self.handler_active = false;
    }

    /// meta_udev_resume()
    pub fn resume(&mut self) {
        self.handler_active = true;
    }
}

/// meta_udev_backlight_find_type(): first device whose sysfs "type" matches.
pub fn backlight_find_type<'a>(devices: &'a [UdevDevice], type_: &str) -> Option<&'a UdevDevice> {
    devices.iter().find(|d| d.sysfs_attr("type") == Some(type_))
}

/// meta_udev_backlight_find_for_connector(): a "raw" backlight whose drm
/// connector parent's name ends with `-<connector_name>` and is enabled.
pub fn backlight_find_for_connector<'a>(
    devices: &'a [UdevDevice],
    connector_name: &str,
) -> Option<&'a UdevDevice> {
    let mut connector_suffix = String::from("-");
    connector_suffix.push_str(connector_name);

    for device in devices {
        // Only raw backlight interfaces.
        if device.sysfs_attr("type") != Some("raw") {
            continue;
        }
        let parent = match device.parent_with_subsystem_devtype("drm", "drm_connector") {
            Some(p) => p,
            None => continue,
        };
        // Name is `card[n]-[connector-name]`; check the suffix.
        if !parent.name.ends_with(&connector_suffix) {
            continue;
        }
        // Connector must be enabled.
        if parent.sysfs_attr("enabled") != Some("enabled") {
            continue;
        }
        return Some(device);
    }
    None
}

/// meta_udev_backlight_find(): choose a backlight device for a connector.
///
/// For internal monitors prefers firmware -> platform, then a connector match,
/// then falls back to the first raw interface. The GUdevClient
/// query-by-subsystem("backlight") is stubbed — the caller supplies `devices`.
pub fn backlight_find<'a>(
    devices: &'a [UdevDevice],
    connector_name: &str,
    is_internal: bool,
) -> Option<&'a UdevDevice> {
    if devices.is_empty() {
        return None;
    }
    if is_internal {
        if let Some(d) = backlight_find_type(devices, "firmware") {
            return Some(d);
        }
        if let Some(d) = backlight_find_type(devices, "platform") {
            return Some(d);
        }
    }
    if let Some(d) = backlight_find_for_connector(devices, connector_name) {
        return Some(d);
    }
    if is_internal {
        return backlight_find_type(devices, "raw");
    }
    None
}
