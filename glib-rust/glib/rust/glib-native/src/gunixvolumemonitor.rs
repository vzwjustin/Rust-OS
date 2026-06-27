//! GUnixVolumeMonitor matching `gio/gunixvolumemonitor.h`.
//! A Unix volume monitor. In this no_std port we model it with
//! a registry of Unix volumes.
//! Fully `no_std` compatible using `alloc`.

use crate::gunixvolume::UnixVolume;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use spin::Mutex;

/// A Unix volume monitor (`GUnixVolumeMonitor`).
pub struct UnixVolumeMonitor {
    volumes: Mutex<Vec<UnixVolume>>,
}

impl UnixVolumeMonitor {
    pub fn new() -> Self {
        Self {
            volumes: Mutex::new(Vec::new()),
        }
    }

    pub fn add_volume(&self, volume: UnixVolume) {
        self.volumes.lock().push(volume);
    }

    pub fn volume_count(&self) -> usize {
        self.volumes.lock().len()
    }

    pub fn get_volume_names(&self) -> Vec<String> {
        self.volumes.lock().iter().map(|v| v.get_name()).collect()
    }

    pub fn mounted_count(&self) -> usize {
        self.volumes
            .lock()
            .iter()
            .filter(|v| v.is_mounted())
            .count()
    }
}

impl Default for UnixVolumeMonitor {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_add_count() {
        let m = UnixVolumeMonitor::new();
        let v = UnixVolume::new("USB", "/dev/sdb1");
        v.mount("/mnt/usb");
        m.add_volume(v);
        m.add_volume(UnixVolume::new("CD", "/dev/sr0"));
        assert_eq!(m.volume_count(), 2);
        assert_eq!(m.mounted_count(), 1);
    }
}
