//! GUnionVolumeMonitor matching `gio/gunionvolumemonitor.h`.
//! A union volume monitor that aggregates multiple monitors. In this
//! no_std port we model it wrapping a NativeVolumeMonitor.
//! Fully `no_std` compatible using `alloc`.

use crate::gnativevolumemonitor::NativeVolumeMonitor;
use alloc::string::String;
use alloc::vec::Vec;

/// A union volume monitor (`GUnionVolumeMonitor`).
pub struct UnionVolumeMonitor {
    monitors: Vec<NativeVolumeMonitor>,
}

impl UnionVolumeMonitor {
    pub fn new() -> Self {
        Self {
            monitors: Vec::new(),
        }
    }
    pub fn add_monitor(&mut self, monitor: NativeVolumeMonitor) {
        self.monitors.push(monitor);
    }
    pub fn volume_count(&self) -> usize {
        self.monitors.iter().map(|m| m.volume_count()).sum()
    }
    pub fn mount_count(&self) -> usize {
        self.monitors.iter().map(|m| m.mount_count()).sum()
    }
    pub fn get_all_volumes(&self) -> Vec<String> {
        self.monitors.iter().flat_map(|m| m.get_volumes()).collect()
    }
    pub fn get_all_mounts(&self) -> Vec<String> {
        self.monitors.iter().flat_map(|m| m.get_mounts()).collect()
    }
}

impl Default for UnionVolumeMonitor {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_union() {
        let mut u = UnionVolumeMonitor::new();
        let m1 = NativeVolumeMonitor::new();
        m1.add_volume("sda1");
        let m2 = NativeVolumeMonitor::new();
        m2.add_volume("sdb1");
        m2.add_volume("sdc1");
        u.add_monitor(m1);
        u.add_monitor(m2);
        assert_eq!(u.volume_count(), 3);
        assert_eq!(u.get_all_volumes().len(), 3);
    }
}
