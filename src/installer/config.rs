//! Write post-install configuration onto the target root filesystem.

use alloc::format;
use alloc::string::String;

use super::format::Ext4Volume;
use super::plan::{InstallPlan, PartitionLayout};
use crate::drivers::storage::StorageError;

/// Write fstab, hostname, passwd, timezone stub, and install marker.
pub fn write_install_config(
    volume: &mut Ext4Volume,
    plan: &InstallPlan,
    layout: &PartitionLayout,
) -> Result<(), StorageError> {
    let root_part = format!(
        "/dev/sd{}p2",
        (b'a' + (layout.device_id as u8).min(25)) as char
    );
    let efi_part = format!(
        "/dev/sd{}p1",
        (b'a' + (layout.device_id as u8).min(25)) as char
    );

    let swap_part = if plan.include_swap {
        format!(
            "/dev/sd{}p3",
            (b'a' + (layout.device_id as u8).min(25)) as char
        )
    } else {
        String::new()
    };
    let swap_line = if plan.include_swap {
        format!("{swap} none swap sw 0 0\n", swap = swap_part)
    } else {
        String::new()
    };
    let fstab = format!(
        "# RustOS installer generated fstab\n{root} / ext4 defaults 0 1\n{efi} /boot/efi vfat umask=0077 0 1\n{swap}",
        root = root_part,
        efi = efi_part,
        swap = swap_line
    );
    volume.write_file("/etc/fstab", fstab.as_bytes())?;

    let hostname = format!("{}\n", plan.hostname);
    volume.write_file("/etc/hostname", hostname.as_bytes())?;

    let hosts = format!("127.0.0.1\tlocalhost\n127.0.1.1\t{}\n", plan.hostname);
    volume.write_file("/etc/hosts", hosts.as_bytes())?;

    let passwd = format!(
        "root:x:0:0:root:/root:/bin/sh\n{}:x:1000:1000:{}:/home/{}:/bin/sh\n",
        plan.username, plan.full_name, plan.username
    );
    volume.write_file("/etc/passwd", passwd.as_bytes())?;

    let shadow = format!(
        "root:!:19000:0:99999:7:::\n{}:{}:19000:0:99999:7:::\n",
        plan.username, plan.password_hash
    );
    volume.write_file("/etc/shadow", shadow.as_bytes())?;

    let timezone = format!("{}\n", plan.timezone);
    volume.write_file("/etc/timezone", timezone.as_bytes())?;
    let _localtime = format!("/usr/share/zoneinfo/{}", plan.timezone);
    let swap_marker = if plan.include_swap {
        swap_part.as_str()
    } else {
        ""
    };
    let installed = format!(
        "installed=true\nhostname={}\nusername={}\nlanguage={}\nroot_part={}\nefi_part={}\nroot_fs=ext4\nefi_fs=vfat\nswap_part={}\n",
        plan.hostname, plan.username, plan.language, root_part, efi_part, swap_marker
    );
    volume.write_file("/etc/rustos-installed", installed.as_bytes())?;

    Ok(())
}
