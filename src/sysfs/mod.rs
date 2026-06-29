//! sysfs virtual filesystem mounted at `/sys`.
//!
//! Exposes kobject hierarchies (devices, buses, classes) with attribute read/write
//! backed by live kernel state (PCI enumeration, SMP CPU online flags, power control).

extern crate alloc;

use alloc::format;
use alloc::string::{String, ToString};
use alloc::sync::Arc;
use alloc::vec::Vec;

use crate::vfs::{DirEntry, InodeOps, InodeType, Stat, VfsError, VfsResult};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct PciLoc {
    bus: u8,
    device: u8,
    function: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SysfsAttrKind {
    PciVendor(PciLoc),
    PciDevice(PciLoc),
    PciClass(PciLoc),
    PciRevision(PciLoc),
    PciSubsystemVendor(PciLoc),
    PciSubsystemDevice(PciLoc),
    PciName(PciLoc),
    PciUevent(PciLoc),
    PciModalias(PciLoc),
    CpuOnline(u32),
    PowerState,
}

fn pci_dev(loc: PciLoc) -> Option<crate::pci::PciDevice> {
    crate::pci::get_all_devices()
        .into_iter()
        .find(|d| d.bus == loc.bus && d.device == loc.device && d.function == loc.function)
}

fn pci_dev_name(loc: PciLoc) -> String {
    format!("0000:{:02x}:{:02x}.{}", loc.bus, loc.device, loc.function)
}

fn class_dir_name(class: u8) -> &'static str {
    match class {
        0x01 => "block",
        0x02 => "net",
        0x03 => "graphics",
        0x04 => "sound",
        0x05 => "memory",
        0x06 => "bridge",
        0x07 => "communication",
        0x08 => "misc",
        0x09 => "input",
        0x0b => "processor",
        0x0c => "serial",
        _ => "misc",
    }
}

struct SysfsAttrInode {
    ino: u64,
    kind: SysfsAttrKind,
    mode: u32,
}

impl SysfsAttrInode {
    fn new(ino: u64, kind: SysfsAttrKind, mode: u32) -> Arc<Self> {
        Arc::new(Self { ino, kind, mode })
    }

    fn read_content(&self) -> String {
        match self.kind {
            SysfsAttrKind::PciVendor(loc) => pci_dev(loc)
                .map(|d| format!("0x{:04x}\n", d.vendor_id))
                .unwrap_or_else(|| String::from("0x0000\n")),
            SysfsAttrKind::PciDevice(loc) => pci_dev(loc)
                .map(|d| format!("0x{:04x}\n", d.device_id))
                .unwrap_or_else(|| String::from("0x0000\n")),
            SysfsAttrKind::PciClass(loc) => pci_dev(loc)
                .map(|d| {
                    format!(
                        "0x{:06x}\n",
                        (d.class as u32) << 16 | (d.subclass as u32) << 8 | d.prog_if as u32
                    )
                })
                .unwrap_or_else(|| String::from("0x000000\n")),
            SysfsAttrKind::PciRevision(loc) => pci_dev(loc)
                .map(|d| format!("0x{:02x}\n", d.revision_id))
                .unwrap_or_else(|| String::from("0x00\n")),
            SysfsAttrKind::PciSubsystemVendor(loc) => pci_dev(loc)
                .map(|d| format!("0x{:04x}\n", d.subsystem_vendor_id))
                .unwrap_or_else(|| String::from("0x0000\n")),
            SysfsAttrKind::PciSubsystemDevice(loc) => pci_dev(loc)
                .map(|d| format!("0x{:04x}\n", d.subsystem_id))
                .unwrap_or_else(|| String::from("0x0000\n")),
            SysfsAttrKind::PciName(loc) => pci_dev(loc)
                .map(|d| format!("{}\n", d.name))
                .unwrap_or_else(|| String::from("Unknown\n")),
            SysfsAttrKind::PciUevent(loc) => {
                let Some(d) = pci_dev(loc) else {
                    return String::from("DRIVER=\n");
                };
                format!(
                    "DRIVER=\nPCI_CLASS={:06X}\nPCI_ID={:04X}:{:04X}\nPCI_SUBSYS_ID={:04X}:{:04X}\nPCI_SLOT_NAME={}\nMODALIAS=pci:v{:08X}d{:08X}sv{:08X}sd{:08X}bc{:02X}sc{:02X}i{:02X}\n",
                    (d.class as u32) << 16 | (d.subclass as u32) << 8 | d.prog_if as u32,
                    d.vendor_id,
                    d.device_id,
                    d.subsystem_vendor_id,
                    d.subsystem_id,
                    pci_dev_name(loc),
                    d.vendor_id as u32,
                    d.device_id as u32,
                    d.subsystem_vendor_id as u32,
                    d.subsystem_id as u32,
                    d.class,
                    d.subclass,
                    d.prog_if,
                )
            }
            SysfsAttrKind::PciModalias(loc) => {
                let Some(d) = pci_dev(loc) else {
                    return String::new();
                };
                format!(
                    "pci:v{:08X}d{:08X}sv{:08X}sd{:08X}bc{:02X}sc{:02X}i{:02X}\n",
                    d.vendor_id as u32,
                    d.device_id as u32,
                    d.subsystem_vendor_id as u32,
                    d.subsystem_id as u32,
                    d.class,
                    d.subclass,
                    d.prog_if,
                )
            }
            SysfsAttrKind::CpuOnline(cpu_id) => {
                if crate::smp::is_cpu_online(cpu_id) {
                    String::from("1\n")
                } else {
                    String::from("0\n")
                }
            }
            SysfsAttrKind::PowerState => String::from("freeze mem disk off\n"),
        }
    }

    fn write_content(&self, buf: &[u8]) -> VfsResult<usize> {
        let text = core::str::from_utf8(buf).map_err(|_| VfsError::InvalidArgument)?;
        let trimmed = text.trim();

        match self.kind {
            SysfsAttrKind::CpuOnline(cpu_id) => {
                if cpu_id == 0 {
                    return Err(VfsError::PermissionDenied);
                }
                match trimmed {
                    "0" => {
                        crate::smp::mark_cpu_offline(cpu_id);
                        Ok(buf.len())
                    }
                    "1" => {
                        crate::smp::mark_cpu_online(cpu_id);
                        Ok(buf.len())
                    }
                    _ => Err(VfsError::InvalidArgument),
                }
            }
            SysfsAttrKind::PowerState => match trimmed {
                "off" => {
                    let _ = crate::kernel::shutdown();
                    Ok(buf.len())
                }
                "mem" => {
                    crate::serial_println!("[sysfs] Suspending CPU...");
                    unsafe {
                        core::arch::asm!("hlt", options(nomem, nostack));
                    }
                    Ok(buf.len())
                }
                "freeze" | "disk" => Ok(buf.len()),
                _ => Err(VfsError::InvalidArgument),
            },
            _ => Err(VfsError::ReadOnly),
        }
    }
}

impl InodeOps for SysfsAttrInode {
    fn read_at(&self, offset: u64, buf: &mut [u8]) -> VfsResult<usize> {
        let content = self.read_content();
        copy_slice(content.as_bytes(), offset, buf)
    }

    fn write_at(&self, offset: u64, buf: &[u8]) -> VfsResult<usize> {
        if offset != 0 {
            return Err(VfsError::InvalidArgument);
        }
        self.write_content(buf)
    }

    fn stat(&self) -> VfsResult<Stat> {
        let size = self.read_content().len() as u64;
        Ok(Stat {
            ino: self.ino,
            inode_type: InodeType::File,
            size,
            blksize: 4096,
            blocks: (size + 511) / 512,
            mode: self.mode,
            nlink: 1,
            uid: 0,
            gid: 0,
            rdev: 0,
            atime: 0,
            mtime: 0,
            ctime: 0,
        })
    }

    fn truncate(&self, _size: u64) -> VfsResult<()> {
        Err(VfsError::NotSupported)
    }

    fn sync(&self) -> VfsResult<()> {
        Ok(())
    }

    fn lookup(&self, _name: &str) -> VfsResult<Arc<dyn InodeOps>> {
        Err(VfsError::NotDirectory)
    }

    fn create(
        &self,
        _name: &str,
        _inode_type: InodeType,
        _mode: u32,
    ) -> VfsResult<Arc<dyn InodeOps>> {
        Err(VfsError::ReadOnly)
    }

    fn unlink(&self, _name: &str) -> VfsResult<()> {
        Err(VfsError::ReadOnly)
    }

    fn link(&self, _name: &str, _target: Arc<dyn InodeOps>) -> VfsResult<()> {
        Err(VfsError::ReadOnly)
    }

    fn rename(
        &self,
        _old_name: &str,
        _new_dir: Arc<dyn InodeOps>,
        _new_name: &str,
    ) -> VfsResult<()> {
        Err(VfsError::ReadOnly)
    }

    fn readdir(&self) -> VfsResult<Vec<DirEntry>> {
        Err(VfsError::NotDirectory)
    }

    fn inode_type(&self) -> InodeType {
        InodeType::File
    }

    fn attach_child(&self, _name: &str, _child: Arc<dyn InodeOps>) -> VfsResult<()> {
        Err(VfsError::NotDirectory)
    }
}

fn copy_slice(bytes: &[u8], offset: u64, buf: &mut [u8]) -> VfsResult<usize> {
    let start = offset as usize;
    if start >= bytes.len() {
        return Ok(0);
    }
    let end = core::cmp::min(start + buf.len(), bytes.len());
    let n = end - start;
    buf[..n].copy_from_slice(&bytes[start..end]);
    Ok(n)
}

fn attach_attr(
    dir: &Arc<dyn InodeOps>,
    name: &str,
    kind: SysfsAttrKind,
    mode: u32,
    ino: u64,
) -> VfsResult<()> {
    dir.attach_child(name, SysfsAttrInode::new(ino, kind, mode))
}

fn attach_pci_kobject(
    parent: &Arc<dyn InodeOps>,
    loc: PciLoc,
    ino: &mut u64,
) -> VfsResult<Arc<dyn InodeOps>> {
    let dev_name = pci_dev_name(loc);
    parent.create(&dev_name, InodeType::Directory, 0o755)?;
    let dev_dir = parent.lookup(&dev_name)?;

    let attrs = [
        (SysfsAttrKind::PciVendor(loc), "vendor", 0o444),
        (SysfsAttrKind::PciDevice(loc), "device", 0o444),
        (SysfsAttrKind::PciClass(loc), "class", 0o444),
        (SysfsAttrKind::PciRevision(loc), "revision", 0o444),
        (
            SysfsAttrKind::PciSubsystemVendor(loc),
            "subsystem_vendor",
            0o444,
        ),
        (
            SysfsAttrKind::PciSubsystemDevice(loc),
            "subsystem_device",
            0o444,
        ),
        (SysfsAttrKind::PciName(loc), "name", 0o444),
        (SysfsAttrKind::PciUevent(loc), "uevent", 0o444),
        (SysfsAttrKind::PciModalias(loc), "modalias", 0o444),
    ];

    for (kind, name, mode) in attrs {
        *ino += 1;
        attach_attr(&dev_dir, name, kind, mode, *ino)?;
    }

    Ok(dev_dir)
}

fn install_pci_devices(
    bus_pci_devices: &Arc<dyn InodeOps>,
    devices_root: &Arc<dyn InodeOps>,
    class_root: &Arc<dyn InodeOps>,
    ino: &mut u64,
) -> VfsResult<()> {
    let pci_devices = crate::pci::get_all_devices();
    if pci_devices.is_empty() {
        return Ok(());
    }

    devices_root.create("pci0000:00", InodeType::Directory, 0o755)?;
    let pci_domain = devices_root.lookup("pci0000:00")?;

    let mut class_dirs: alloc::collections::BTreeMap<String, Arc<dyn InodeOps>> =
        alloc::collections::BTreeMap::new();

    for dev in &pci_devices {
        let loc = PciLoc {
            bus: dev.bus,
            device: dev.device,
            function: dev.function,
        };

        *ino += 1;
        let kobj = attach_pci_kobject(&pci_domain, loc, ino)?;

        *ino += 1;
        attach_pci_kobject(bus_pci_devices, loc, ino)?;

        let class_name = class_dir_name(dev.class);
        let class_dir = if let Some(existing) = class_dirs.get(class_name) {
            Arc::clone(existing)
        } else {
            class_root.create(class_name, InodeType::Directory, 0o755)?;
            let created = class_root.lookup(class_name)?;
            class_dirs.insert(class_name.to_string(), Arc::clone(&created));
            created
        };

        let link_name = dev
            .name
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
                    c
                } else {
                    '_'
                }
            })
            .collect::<String>();
        let link_name = if link_name.is_empty() {
            pci_dev_name(loc).replace(':', "_")
        } else {
            link_name
        };

        let target = format!(
            "../../devices/pci0000:00/{dev_name}",
            dev_name = pci_dev_name(loc)
        );
        class_dir.create(&link_name, InodeType::Symlink, 0o777)?;
        let link = class_dir.lookup(&link_name)?;
        link.write_at(0, target.as_bytes())?;

        let _ = kobj;
    }

    Ok(())
}

fn install_cpu_nodes(system: &Arc<dyn InodeOps>, ino: &mut u64) -> VfsResult<()> {
    system.create("cpu", InodeType::Directory, 0o755)?;
    let cpu_root = system.lookup("cpu")?;

    let count = crate::smp::cpu_count().max(1);
    for cpu_id in 0..count {
        let dir_name = format!("cpu{cpu_id}");
        cpu_root.create(&dir_name, InodeType::Directory, 0o755)?;
        let cpu_dir = cpu_root.lookup(&dir_name)?;
        *ino += 1;
        attach_attr(
            &cpu_dir,
            "online",
            SysfsAttrKind::CpuOnline(cpu_id),
            if cpu_id == 0 { 0o444 } else { 0o644 },
            *ino,
        )?;
    }

    Ok(())
}

/// Populate `/sys` with device, bus, and class kobjects.
pub fn install_sysfs(root: Arc<dyn InodeOps>) -> VfsResult<()> {
    let sys = root.lookup("sys")?;
    let mut ino = 30_000u64;

    sys.create("bus", InodeType::Directory, 0o755)?;
    let bus = sys.lookup("bus")?;
    bus.create("pci", InodeType::Directory, 0o755)?;
    let pci_bus = bus.lookup("pci")?;
    pci_bus.create("devices", InodeType::Directory, 0o755)?;
    let pci_bus_devices = pci_bus.lookup("devices")?;

    sys.create("devices", InodeType::Directory, 0o755)?;
    let devices = sys.lookup("devices")?;
    devices.create("system", InodeType::Directory, 0o755)?;
    let system = devices.lookup("system")?;
    install_cpu_nodes(&system, &mut ino)?;

    sys.create("class", InodeType::Directory, 0o755)?;
    let class = sys.lookup("class")?;

    install_pci_devices(&pci_bus_devices, &devices, &class, &mut ino)?;

    sys.create("power", InodeType::Directory, 0o755)?;
    let power = sys.lookup("power")?;
    ino += 1;
    attach_attr(&power, "state", SysfsAttrKind::PowerState, 0o644, ino)?;

    crate::trace::install_sysfs(&sys, &mut ino)?;

    Ok(())
}
