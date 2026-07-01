//! MetaMonitorConfigStore ported from GNOME Mutter's
//! src/core/meta-monitor-config-store.c
//!
//! MetaMonitorConfigStore persists monitor configurations to disk (in
//! Mutter, to ~/.config/monitors.xml). It stores per-monitor-set
//! configurations keyed by the monitor vendor/product/serial identifiers.
//!
//! In the kernel, filesystem access is available through the kernel's VFS.
//! The config store uses a simple in-memory representation that can be
//! serialized/deserialized to a file. The XML format is simplified to a
//! line-based key-value format suitable for the kernel's no_std environment.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/core/meta-monitor-config-store.c

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::string::ToString;
use alloc::vec::Vec;

use super::monitor_config_manager::{ConfigSource, MonitorConfig, MonitorsConfig};
use super::monitor_manager::LayoutMode;
use crate::mutter_port::backends::logical_monitor::MonitorTransform;

/// A stored monitor configuration entry, keyed by the monitor set identity.
/// Mirrors the structure of monitors.xml <configuration> elements.
#[derive(Debug, Clone)]
pub struct StoredConfig {
    /// Stable identifier for this monitor set (hash of connector/vendor/product/serial).
    pub key: String,
    /// The stored configuration.
    pub config: MonitorsConfig,
}

/// The monitor config store. Mirrors MetaMonitorConfigStore.
///
/// In Mutter this reads/writes ~/.config/monitors.xml. Here we keep an
/// in-memory map; serialization to/from a string is provided so the
/// kernel VFS layer can read/write the file.
#[derive(Debug)]
pub struct MetaMonitorConfigStore {
    /// Stored configurations keyed by monitor set id.
    configs: BTreeMap<String, StoredConfig>,
}

impl MetaMonitorConfigStore {
    /// Create a new empty config store. Mirrors meta_monitor_config_store_new().
    pub fn new() -> Self {
        MetaMonitorConfigStore {
            configs: BTreeMap::new(),
        }
    }

    /// Add or replace a stored configuration. Mirrors
    /// meta_monitor_config_store_add().
    pub fn add(&mut self, key: &str, config: MonitorsConfig) {
        let stored = StoredConfig {
            key: String::from(key),
            config,
        };
        self.configs.insert(String::from(key), stored);
    }

    /// Look up a stored configuration by key. Mirrors
    /// meta_monitor_config_store_lookup().
    pub fn lookup(&self, key: &str) -> Option<&MonitorsConfig> {
        self.configs.get(key).map(|s| &s.config)
    }

    /// Remove a stored configuration. Mirrors
    /// meta_monitor_config_store_remove().
    pub fn remove(&mut self, key: &str) -> bool {
        self.configs.remove(key).is_some()
    }

    /// Number of stored configurations.
    pub fn count(&self) -> usize {
        self.configs.len()
    }

    /// All stored configuration keys.
    pub fn keys(&self) -> impl Iterator<Item = &String> {
        self.configs.keys()
    }

    /// Generate a stable key for a set of monitors. Mirrors
    /// meta_monitor_config_store_generate_key().
    ///
    /// The key is a concatenation of connector:vendor:product:serial for
    /// each monitor, separated by '|'.
    pub fn generate_key(monitors: &[(&str, &str, &str, &str)]) -> String {
        let mut key = String::new();
        for (i, (connector, vendor, product, serial)) in monitors.iter().enumerate() {
            if i > 0 {
                key.push('|');
            }
            key.push_str(connector);
            key.push(':');
            key.push_str(vendor);
            key.push(':');
            key.push_str(product);
            key.push(':');
            key.push_str(serial);
        }
        key
    }

    // ── Serialization ─────────────────────────────────────────────────

    /// Serialize all stored configurations to a string. Mirrors the
    /// monitors.xml write path. The format is line-based:
    /// ```text
    /// [config:<key>]
    /// layout=physical
    /// linear=true
    /// monitor=<connector>,<mode_id>,<x>,<y>,<primary>,<transform>,<scale>
    /// ...
    /// ```
    pub fn serialize(&self) -> String {
        let mut out = String::new();

        for stored in self.configs.values() {
            out.push_str("[config:");
            out.push_str(&stored.key);
            out.push_str("]\n");

            match stored.config.layout_mode {
                LayoutMode::Physical => out.push_str("layout=physical\n"),
                LayoutMode::Logical => out.push_str("layout=logical\n"),
            }

            out.push_str(if stored.config.linear {
                "linear=true\n"
            } else {
                "linear=false\n"
            });

            for mc in &stored.config.monitor_configs {
                out.push_str("monitor=");
                out.push_str(&mc.connector);
                out.push(',');
                out.push_str(&mc.mode_id.to_string());
                out.push(',');
                out.push_str(&mc.x.to_string());
                out.push(',');
                out.push_str(&mc.y.to_string());
                out.push(',');
                out.push_str(if mc.primary { "1" } else { "0" });
                out.push(',');
                out.push_str(&transform_to_string(mc.transform));
                out.push(',');
                out.push_str(&mc.scale.to_string());
                out.push('\n');
            }
            out.push('\n');
        }

        out
    }

    /// Deserialize configurations from a string. Mirrors the monitors.xml
    /// read path. Returns the number of configs loaded.
    pub fn deserialize(&mut self, data: &str) -> usize {
        let mut count = 0;
        let mut current_key = String::new();
        let mut current_layout = LayoutMode::Physical;
        let mut current_linear = true;
        let mut current_monitors: Vec<MonitorConfig> = Vec::new();
        let mut in_config = false;

        for line in data.lines() {
            let line = line.trim();

            if line.starts_with("[config:") && line.ends_with(']') {
                // Save previous config if any.
                if in_config && !current_key.is_empty() {
                    let config = MonitorsConfig {
                        id: 0,
                        monitor_configs: core::mem::take(&mut current_monitors),
                        layout_mode: current_layout,
                        linear: current_linear,
                        source: ConfigSource::Stored,
                    };
                    self.add(&current_key, config);
                    count += 1;
                }

                // Start new config.
                let key = &line["[config:".len()..line.len() - 1];
                current_key = String::from(key);
                current_layout = LayoutMode::Physical;
                current_linear = true;
                current_monitors.clear();
                in_config = true;
                continue;
            }

            if !in_config {
                continue;
            }

            if let Some(val) = line.strip_prefix("layout=") {
                current_layout = match val {
                    "logical" => LayoutMode::Logical,
                    _ => LayoutMode::Physical,
                };
            } else if let Some(val) = line.strip_prefix("linear=") {
                current_linear = val == "true";
            } else if let Some(rest) = line.strip_prefix("monitor=") {
                if let Some(mc) = parse_monitor_line(rest) {
                    current_monitors.push(mc);
                }
            }
        }

        // Save last config.
        if in_config && !current_key.is_empty() {
            let config = MonitorsConfig {
                id: 0,
                monitor_configs: current_monitors,
                layout_mode: current_layout,
                linear: current_linear,
                source: ConfigSource::Stored,
            };
            self.add(&current_key, config);
            count += 1;
        }

        count
    }
}

impl Default for MetaMonitorConfigStore {
    fn default() -> Self {
        Self::new()
    }
}

fn transform_to_string(t: MonitorTransform) -> &'static str {
    match t {
        MonitorTransform::Normal => "normal",
        MonitorTransform::Rotate90 => "rotate90",
        MonitorTransform::Rotate180 => "rotate180",
        MonitorTransform::Rotate270 => "rotate270",
        MonitorTransform::Flipped => "flipped",
        MonitorTransform::FlippedRotate90 => "flipped90",
        MonitorTransform::FlippedRotate180 => "flipped180",
        MonitorTransform::FlippedRotate270 => "flipped270",
    }
}

fn transform_from_string(s: &str) -> MonitorTransform {
    match s {
        "rotate90" => MonitorTransform::Rotate90,
        "rotate180" => MonitorTransform::Rotate180,
        "rotate270" => MonitorTransform::Rotate270,
        "flipped" => MonitorTransform::Flipped,
        "flipped90" => MonitorTransform::FlippedRotate90,
        "flipped180" => MonitorTransform::FlippedRotate180,
        "flipped270" => MonitorTransform::FlippedRotate270,
        _ => MonitorTransform::Normal,
    }
}

fn parse_monitor_line(line: &str) -> Option<MonitorConfig> {
    let parts: Vec<&str> = line.split(',').collect();
    if parts.len() < 7 {
        return None;
    }

    let connector = String::from(parts[0]);
    let mode_id: u32 = parts[1].parse().ok()?;
    let x: i32 = parts[2].parse().ok()?;
    let y: i32 = parts[3].parse().ok()?;
    let primary = parts[4] == "1";
    let transform = transform_from_string(parts[5]);
    let scale: f32 = parts[6].parse().ok()?;

    Some(MonitorConfig {
        connector,
        vendor: String::new(),
        product: String::new(),
        serial: String::new(),
        mode_id,
        enabled: true,
        primary,
        x,
        y,
        transform,
        scale,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_test_config(connector: &str, mode_id: u32, x: i32, primary: bool) -> MonitorsConfig {
        let mut mc = MonitorConfig::new(connector, mode_id);
        mc.x = x;
        mc.primary = primary;
        MonitorsConfig {
            id: 1,
            monitor_configs: vec![mc],
            layout_mode: LayoutMode::Physical,
            linear: true,
            source: ConfigSource::Stored,
        }
    }

    #[test]
    fn test_empty_store() {
        let store = MetaMonitorConfigStore::new();
        assert_eq!(store.count(), 0);
    }

    #[test]
    fn test_add_and_lookup() {
        let mut store = MetaMonitorConfigStore::new();
        let config = make_test_config("DP-1", 1, 0, true);
        store.add("DP-1:vendor:product:serial", config);

        assert_eq!(store.count(), 1);
        assert!(store.lookup("DP-1:vendor:product:serial").is_some());
        assert!(store.lookup("nonexistent").is_none());
    }

    #[test]
    fn test_remove() {
        let mut store = MetaMonitorConfigStore::new();
        let config = make_test_config("DP-1", 1, 0, true);
        store.add("key1", config);

        assert!(store.remove("key1"));
        assert_eq!(store.count(), 0);
        assert!(!store.remove("key1"));
    }

    #[test]
    fn test_generate_key() {
        let key = MetaMonitorConfigStore::generate_key(&[
            ("DP-1", "DEL", "Monitor", "12345"),
            ("eDP-1", "BOE", "Panel", "67890"),
        ]);
        assert_eq!(key, "DP-1:DEL:Monitor:12345|eDP-1:BOE:Panel:67890");
    }

    #[test]
    fn test_serialize_deserialize() {
        let mut store = MetaMonitorConfigStore::new();
        let config = make_test_config("DP-1", 1, 0, true);
        store.add("key1", config);

        let serialized = store.serialize();
        assert!(!serialized.is_empty());
        assert!(serialized.contains("[config:key1]"));

        let mut store2 = MetaMonitorConfigStore::new();
        let count = store2.deserialize(&serialized);
        assert_eq!(count, 1);
        assert!(store2.lookup("key1").is_some());

        let loaded = store2.lookup("key1").unwrap();
        assert_eq!(loaded.monitor_configs[0].connector, "DP-1");
        assert_eq!(loaded.monitor_configs[0].mode_id, 1);
        assert!(loaded.monitor_configs[0].primary);
    }

    #[test]
    fn test_serialize_multiple_configs() {
        let mut store = MetaMonitorConfigStore::new();
        store.add("key1", make_test_config("DP-1", 1, 0, true));
        store.add("key2", make_test_config("eDP-1", 2, 1920, false));

        let serialized = store.serialize();

        let mut store2 = MetaMonitorConfigStore::new();
        let count = store2.deserialize(&serialized);
        assert_eq!(count, 2);
    }

    #[test]
    fn test_deserialize_empty() {
        let mut store = MetaMonitorConfigStore::new();
        assert_eq!(store.deserialize(""), 0);
        assert_eq!(store.count(), 0);
    }

    #[test]
    fn test_deserialize_malformed() {
        let mut store = MetaMonitorConfigStore::new();
        let data = "random text\nno config here\n";
        assert_eq!(store.deserialize(data), 0);
    }

    #[test]
    fn test_roundtrip_with_transform() {
        let mut store = MetaMonitorConfigStore::new();
        let mut mc = MonitorConfig::new("DP-1", 1);
        mc.transform = MonitorTransform::Rotate90;
        mc.scale = 2.0;
        mc.x = 100;
        mc.y = 200;
        let config = MonitorsConfig {
            id: 1,
            monitor_configs: vec![mc],
            layout_mode: LayoutMode::Logical,
            linear: false,
            source: ConfigSource::Stored,
        };
        store.add("key1", config);

        let serialized = store.serialize();
        let mut store2 = MetaMonitorConfigStore::new();
        store2.deserialize(&serialized);

        let loaded = store2.lookup("key1").unwrap();
        assert_eq!(loaded.layout_mode, LayoutMode::Logical);
        assert!(!loaded.linear);
        assert_eq!(
            loaded.monitor_configs[0].transform,
            MonitorTransform::Rotate90
        );
        assert_eq!(loaded.monitor_configs[0].scale, 2.0);
        assert_eq!(loaded.monitor_configs[0].x, 100);
        assert_eq!(loaded.monitor_configs[0].y, 200);
    }
}
