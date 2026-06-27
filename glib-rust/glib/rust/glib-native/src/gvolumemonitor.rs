use alloc::string::{String, ToString};
use alloc::vec::Vec;
use spin::Mutex;

pub struct VolumeEntry {
    pub name: String,
    pub mount_path: Option<String>,
    pub is_mounted: bool,
}

pub struct DriveEntry {
    pub name: String,
    pub volumes: Vec<String>,
}

pub struct VolumeMonitor {
    volumes: Mutex<Vec<VolumeEntry>>,
    drives: Mutex<Vec<DriveEntry>>,
}

impl VolumeMonitor {
    pub fn new() -> Self {
        VolumeMonitor {
            volumes: Mutex::new(Vec::new()),
            drives: Mutex::new(Vec::new()),
        }
    }

    pub fn get_volumes(&self) -> Vec<String> {
        self.volumes.lock().iter().map(|v| v.name.clone()).collect()
    }

    pub fn get_mounts(&self) -> Vec<String> {
        self.volumes
            .lock()
            .iter()
            .filter(|v| v.is_mounted)
            .map(|v| v.name.clone())
            .collect()
    }

    pub fn get_drives(&self) -> Vec<String> {
        self.drives.lock().iter().map(|d| d.name.clone()).collect()
    }

    pub fn add_volume(&self, name: &str) {
        self.volumes.lock().push(VolumeEntry {
            name: name.to_string(),
            mount_path: None,
            is_mounted: false,
        });
    }

    pub fn remove_volume(&self, name: &str) -> bool {
        let mut volumes = self.volumes.lock();
        if let Some(pos) = volumes.iter().position(|v| v.name == name) {
            volumes.remove(pos);
            true
        } else {
            false
        }
    }

    pub fn mount_volume(&self, name: &str, mount_path: &str) -> bool {
        let mut volumes = self.volumes.lock();
        if let Some(v) = volumes.iter_mut().find(|v| v.name == name) {
            v.is_mounted = true;
            v.mount_path = Some(mount_path.to_string());
            true
        } else {
            false
        }
    }

    pub fn unmount_volume(&self, name: &str) -> bool {
        let mut volumes = self.volumes.lock();
        if let Some(v) = volumes.iter_mut().find(|v| v.name == name) {
            v.is_mounted = false;
            v.mount_path = None;
            true
        } else {
            false
        }
    }

    pub fn add_drive(&self, name: &str) {
        self.drives.lock().push(DriveEntry {
            name: name.to_string(),
            volumes: Vec::new(),
        });
    }

    pub fn remove_drive(&self, name: &str) -> bool {
        let mut drives = self.drives.lock();
        if let Some(pos) = drives.iter().position(|d| d.name == name) {
            drives.remove(pos);
            true
        } else {
            false
        }
    }

    pub fn add_volume_to_drive(&self, drive_name: &str, volume_name: &str) -> bool {
        let mut drives = self.drives.lock();
        if let Some(d) = drives.iter_mut().find(|d| d.name == drive_name) {
            d.volumes.push(volume_name.to_string());
            true
        } else {
            false
        }
    }

    pub fn volume_count(&self) -> usize {
        self.volumes.lock().len()
    }

    pub fn drive_count(&self) -> usize {
        self.drives.lock().len()
    }
}

impl Default for VolumeMonitor {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_is_empty() {
        let m = VolumeMonitor::new();
        assert_eq!(m.volume_count(), 0);
        assert_eq!(m.drive_count(), 0);
        assert!(m.get_volumes().is_empty());
        assert!(m.get_mounts().is_empty());
        assert!(m.get_drives().is_empty());
    }

    #[test]
    fn test_add_and_get_volume() {
        let m = VolumeMonitor::new();
        m.add_volume("sda1");
        m.add_volume("sdb1");
        let names = m.get_volumes();
        assert_eq!(names.len(), 2);
        assert!(names.contains(&"sda1".to_string()));
        assert!(names.contains(&"sdb1".to_string()));
    }

    #[test]
    fn test_mount_and_unmount_volume() {
        let m = VolumeMonitor::new();
        m.add_volume("sda1");

        assert!(m.get_mounts().is_empty());

        let mounted = m.mount_volume("sda1", "/mnt/data");
        assert!(mounted);
        let mounts = m.get_mounts();
        assert_eq!(mounts.len(), 1);
        assert_eq!(mounts[0], "sda1");

        let unmounted = m.unmount_volume("sda1");
        assert!(unmounted);
        assert!(m.get_mounts().is_empty());
    }

    #[test]
    fn test_mount_nonexistent_volume_returns_false() {
        let m = VolumeMonitor::new();
        assert!(!m.mount_volume("ghost", "/mnt/ghost"));
        assert!(!m.unmount_volume("ghost"));
    }

    #[test]
    fn test_remove_volume() {
        let m = VolumeMonitor::new();
        m.add_volume("sda1");
        m.add_volume("sdb1");
        assert_eq!(m.volume_count(), 2);

        let removed = m.remove_volume("sda1");
        assert!(removed);
        assert_eq!(m.volume_count(), 1);
        assert_eq!(m.get_volumes()[0], "sdb1");

        let again = m.remove_volume("sda1");
        assert!(!again);
    }

    #[test]
    fn test_add_and_remove_drive() {
        let m = VolumeMonitor::new();
        m.add_drive("sda");
        m.add_drive("sdb");
        assert_eq!(m.drive_count(), 2);

        let removed = m.remove_drive("sda");
        assert!(removed);
        assert_eq!(m.drive_count(), 1);
        assert_eq!(m.get_drives()[0], "sdb");

        assert!(!m.remove_drive("sda"));
    }

    #[test]
    fn test_add_volume_to_drive() {
        let m = VolumeMonitor::new();
        m.add_drive("sda");
        m.add_volume("sda1");
        m.add_volume("sda2");

        assert!(m.add_volume_to_drive("sda", "sda1"));
        assert!(m.add_volume_to_drive("sda", "sda2"));
        assert!(!m.add_volume_to_drive("nonexistent", "sda1"));

        let drives = m.drives.lock();
        let sda = drives.iter().find(|d| d.name == "sda").unwrap();
        assert_eq!(sda.volumes.len(), 2);
        assert!(sda.volumes.contains(&"sda1".to_string()));
        assert!(sda.volumes.contains(&"sda2".to_string()));
    }

    #[test]
    fn test_default_equals_new() {
        let m: VolumeMonitor = Default::default();
        assert_eq!(m.volume_count(), 0);
        assert_eq!(m.drive_count(), 0);
    }

    #[test]
    fn test_get_mounts_only_returns_mounted() {
        let m = VolumeMonitor::new();
        m.add_volume("sda1");
        m.add_volume("sda2");
        m.add_volume("sda3");

        m.mount_volume("sda1", "/");
        m.mount_volume("sda3", "/home");

        let mounts = m.get_mounts();
        assert_eq!(mounts.len(), 2);
        assert!(mounts.contains(&"sda1".to_string()));
        assert!(mounts.contains(&"sda3".to_string()));
        assert!(!mounts.contains(&"sda2".to_string()));
    }

    #[test]
    fn test_volume_and_drive_counts() {
        let m = VolumeMonitor::new();
        assert_eq!(m.volume_count(), 0);
        assert_eq!(m.drive_count(), 0);

        m.add_volume("v1");
        m.add_volume("v2");
        m.add_volume("v3");
        assert_eq!(m.volume_count(), 3);

        m.add_drive("d1");
        m.add_drive("d2");
        assert_eq!(m.drive_count(), 2);

        m.remove_volume("v2");
        assert_eq!(m.volume_count(), 2);

        m.remove_drive("d1");
        assert_eq!(m.drive_count(), 1);
    }
}
