# RustOS Live and Install Media

RustOS ships a live/install ISO that lets you try the desktop from read-only media or install to disk. The build pipeline lives under `scripts/` and is driven by Make targets.

## Building install media

```bash
# Full pipeline: installer rootfs, live squashfs, and ISO tree
make installer-gtk      # zenity + GTK4/libadwaita for graphical wizard
make install-media

# Individual steps
make installer-rootfs    # userspace/installer-rootfs (~30 MB target)
make installer-initramfs # userspace/installer-initramfs.cpio
make live-squashfs       # build/live/filesystem.squashfs
make live-iso            # build/iso/ and build/rustos-live.iso (when xorriso/grub available)
```

Release media:

```bash
make installer-gtk      # zenity + GTK4/libadwaita for graphical wizard
make install-media RELEASE=1
# or
./scripts/build-live-iso.sh --release
```

### Host dependencies

| Tool | Purpose |
|------|---------|
| `cpio` | Pack installer initramfs (required) |
| `mksquashfs` | Compress live rootfs (optional; falls back to `tar.gz`) |
| `xorriso` | Wrap the raw bootimage into an ISO (optional; staged tree + QEMU instructions otherwise) |
| `rsync` | Stage rootfs excluding apk metadata |

Optional Alpine packages for installer initramfs disk tools (install into `userspace/rootfs` with `apk --root`):

- `e2fsprogs` - `mkfs.ext4`, `fsck.ext4`
- `dosfstools` - `mkfs.vfat`
- `busybox` - `mount`, shell (default in rootfs)

## ISO layout

```text
build/iso/
  boot/bootimage-rustos.bin  # Bootloader raw disk image
  boot/initrd.img            # Minimal installer initramfs artifact
  live/filesystem.squashfs   # Compressed live rootfs (or filesystem.tar.gz fallback)
  .disk/info                 # Build metadata
```

When `xorriso` is available, `build/rustos-live.iso` is produced by wrapping the raw bootimage with hard-disk boot emulation. Without `xorriso`, the staged tree is still usable with QEMU:

```bash
qemu-system-x86_64 \
  -drive format=raw,file=build/iso/boot/bootimage-rustos.bin \
  -m 2G -smp 2 \
  -serial stdio \
  -display cocoa
```

## Download and boot

1. Build or obtain `build/rustos-live.iso`.
2. Write to USB (example):

```bash
sudo dd if=build/rustos-live.iso of=/dev/sdX bs=4M status=progress conv=fsync
```

3. Boot the machine from USB. Boot menu selection happens inside RustOS.

## Try vs install

| Mode | Boot parameter | Init behavior |
|------|----------------|---------------|
| **Try (live)** | `rustos.boot=live` | Boots compressed live rootfs; changes are not persisted |
| **Install** | `rustos.boot=install` | Minimal initramfs sets `RUSTOS_BOOT=install` and starts `rustos-installer-ui` |

The installer initramfs PID 1 (`rustos-installer-init`) mounts `proc`, `sys`, `dev`, and `run`, exports `RUSTOS_BOOT=install`, and execs the installer UI.

GRUB entries (when ISO tooling is present):

- **RustOS Live (try)** — `rustos.boot=live`
- **RustOS Install** — `rustos.boot=install`

## QEMU

### Bootable ISO (when built)

```bash
qemu-system-x86_64 \
  -cdrom build/rustos-live.iso \
  -m 2G -smp 2 \
  -serial stdio \
  -display cocoa
```

On Linux, use `-display gtk` instead of `cocoa`.

### Manual bootimage (no ISO tooling)

If only the staged tree exists under `build/iso/`:

```bash
qemu-system-x86_64 \
  -drive format=raw,file=build/iso/boot/bootimage-rustos.bin \
  -m 2G -smp 2 \
  -serial stdio \
  -display cocoa
```

Live try session:

```bash
-append "rustos.boot=live"
```

### Existing disk image workflow

The standard development image still works for day-to-day testing:

```bash
make bootimage && make run
```

## Hardware install notes

1. **Firmware** — Enable UEFI boot for GPT/EFI installs; legacy BIOS uses MBR partitioning.
2. **Disk** — The installer targets block devices matching `/dev/sd?` by default (see `installer.conf`). NVMe devices appear as `/dev/nvme*n*`.
3. **EFI system partition** — Default 512 MB VFAT ESP (`efi_size_mb` in config).
4. **Swap** — Default 2 GB swap partition (`swap_size_mb`).
5. **Root filesystem** — Default ext4 on the remaining disk space.
6. **Network** — DHCP is used when available for post-install updates.
7. **Display** — Install UI expects a framebuffer or serial console; headless installs use autoinstall (below).

Back up important data before partitioning. The installer writes partition tables and formats target disks.

## Autoinstall plan format

Unattended installs read a plan file (default `/tmp/rustos-install.plan`). Paths are defined in `userspace/rootfs/etc/rustos/installer.conf`.

### Example plan

```ini
# rustos-install.plan — autoinstall configuration

version=1
mode=install

# Target disk (required for autoinstall)
disk=/dev/sda

# Host identity
hostname=rustos-desktop
username=rustos
fullname=RustOS User
password_plaintext=changeme
timezone=UTC
locale=en_US.UTF-8

# Partition layout (sizes in MB; root uses remaining space when root_size_mb=0)
efi_size_mb=512
swap_size_mb=2048
root_size_mb=0
root_fs=ext4
efi_fs=vfat

# Optional: wipe partition table before install
wipe_disk=yes

# Post-install
reboot=yes
```

### Kernel Interfaces

The installer coordinates with kernel procfs nodes when they are enabled:

| Path | Purpose |
|------|---------|
| `/proc/rustos/installer/status` | Installer status |
| `/proc/rustos/installer/progress` | Installer progress percentage |
| `/proc/rustos/installer/plan` | Write plan contents |
| `/proc/rustos/installer/mode` | `live` or `install` |
| `/proc/rustos/installer/apply` | Trigger apply (write `1` or `apply`) |

### Supplying a plan at boot

```bash
# From a running installer shell:
cat /path/to/plan > /proc/rustos/installer/plan
echo apply > /proc/rustos/installer/apply
```

The active bootloader raw-image path boots through RustOS' own boot menu. Submit unattended plans through the installer procfs bridge after the installer environment is running.

### Status and logs

During install:

- Status: `/run/rustos-installer.status`
- Log: `/run/rustos-installer.log`
- Installed marker on target: `/etc/rustos-installed`

## Troubleshooting

| Symptom | Check |
|---------|-------|
| Initramfs too large | `make installer-rootfs`; trim copied binaries |
| No squashfs output | Install `squashfs-tools`; tarball fallback is under `build/live/` |
| No ISO file | Install `grub-pc-bin` + `xorriso`; use staged `build/iso/` with QEMU |
| Installer UI missing | Copy `rustos-installer-ui` into `userspace/rootfs/usr/bin/` and rebuild |
| `mkfs.ext4` fails | Install `e2fsprogs` into rootfs (`apk --root userspace/rootfs add e2fsprogs`) |
