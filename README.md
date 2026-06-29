# RustOS

RustOS is an experimental x86_64 operating system kernel written in Rust (`#![no_std]`). It targets modern PC hardware with ACPI/APIC, a full TCP/IP stack, Linux-compatible APIs, a GNOME-oriented desktop path, and tooling to build live/install media. The project is under active development — many subsystems compile and boot in QEMU; end-to-end Linux binary compatibility and hardware deployment are still in progress.

## Highlights

- **Bootable kernel** via the `bootloader` crate and `cargo bootimage` (see `make run`, `make boot-smoke`)
- **glib-native** — in-tree, `no_std` Rust reimplementation of GLib/GObject/GIO (~470 modules under `glib-rust/glib/rust/glib-native/`)
- **GNOME desktop path** — D-Bus, Wayland compositor hooks, Mutter integration, Alpine `gnome-shell` rootfs targets
- **Live & install media** — installer initramfs, squashfs live rootfs, and ISO staging (`make install-media`)
- **Linux compatibility** — 200+ POSIX/Linux API shims plus modern kernel interfaces (see below)
- **Dynamic linking groundwork** — ELF loader and x86_64 dynamic linker for shared-library binaries
- **Package management** — `.deb` / `.rpm` / `.apk` adapters with AR/TAR/GZIP extraction

## Features

### Hardware & platform

| Area | Modules / notes |
|------|-----------------|
| ACPI | RSDP, RSDT/XSDT, MADT, FADT, MCFG, HPET |
| Interrupts | Local APIC, IO-APIC, legacy PIC fallback |
| PCI/PCIe | Enumeration, config space, 500+ device database, hot-plug hooks |
| SMP | APIC IPI, per-CPU data, CPU affinity |
| Timers | PIT, TSC (`src/time.rs`) |
| CPU | CPUID feature detection (`src/arch.rs`) |
| Power | `cpufreq`, `cpuidle`, `power`, EFI helpers |
| NUMA / memory hardware | `numa`, `nvdimm`, `edac`, `memory_hotplug`, `thp`, `hugetlb` |

### Memory, processes & scheduling

- Zone/heap allocation with `linked_list_allocator` and virtual-memory manager (`memory`, `memory_manager`)
- Preemptive scheduler with SMP load balancing (`scheduler/`, `process/`)
- ELF loader and usermode helpers (`elf_loader`, `usermode`)
- Dynamic linker for `PT_DYNAMIC` / GOT / PLT relocations (`process/dynamic_linker.rs`)
- Swap, OOM killer, userfaultfd, memfd_secret
- RCU, softirq/workqueues, futex, epoll, io_uring, aio

### Linux API compatibility

`src/linux_compat/` provides binary-compatible types and hundreds of syscall shims across file, process, time, signal, socket, IPC, TTY, memory, threading, filesystem, and resource categories.

Additional Linux-style kernel subsystems wired in `main.rs`:

`seccomp`, `namespaces`, `cgroup`, `landlock`, `ptrace`, `inotify`, `fanotify`, `pidfd`, `mount_api`, `quota`, `bpf`, `keyring`, `sysv_ipc`, `audit`, `perf_event`, `kprobes`, `trace`, `kexec`, `privileged_syscalls`, `process_vm`, `rseq`, `userfaultfd`, `file_handle`

### Network stack

Full in-kernel TCP/IP (`src/net/`): Ethernet, IPv4, TCP, UDP, sockets, plus ARP, ICMP, DHCP, and DNS helpers under `src/network/`. Drivers for Intel, Realtek, and Broadcom NICs.

### Storage, VFS & filesystems

- VFS layer with RamFS, DevFS, initramfs unpacking (`fs/`, `vfs/`, `initramfs/`)
- sysfs and proc-style nodes (including installer procfs bridge)
- Block I/O layer and I/O scheduler (`block_io`, `io_optimized`)
- Driver coverage: AHCI/NVMe/IDE (`drivers/storage/`), SCSI, VirtIO block

### Device drivers

`src/drivers/` includes network, storage, USB, HID, I2C, SPI, DMA, TTY, thermal, regulator, and VirtIO drivers. PCI hot-plug and device database live under `src/pci/`.

### Graphics, GPU & desktop

- Framebuffer and VGA text mode (`graphics/`, `vga_buffer.rs`, `vga_mode13h.rs`)
- Multi-vendor GPU framework with open-source driver stubs (Nouveau, AMDGPU, i915) under `src/gpu/`
- Desktop window manager and boot UI (`desktop/`, `simple_desktop/`, `boot_ui.rs`)
- GNOME integration: `glib`, `glib_platform`, `glib_spawn`, `gnome`, `mutter`, `dbus`, `wayland`

### Sound

ALSA-style device registry (`src/sound/`).

### Security & crypto

- Ring 0–3 privilege model (`security.rs`)
- Kernel crypto API (`crypto/`)
- KASAN / KCSAN hooks for memory-safety testing

### Package management

`src/package/` — format adapters (`.deb`, `.rpm`, `.apk`, native), compression (gzip/tar), database, and dedicated syscalls (200–206). See `src/package/README.md`.

### Installer & live media

Graphical and headless installer with autoinstall plan support. Builds a minimal installer initramfs, compressed live rootfs, and ISO tree.

```bash
make installer-gtk      # zenity + GTK4 into rootfs
make install-media      # full pipeline
make live-iso           # stage build/iso/
```

See [docs/INSTALLER.md](docs/INSTALLER.md) for layout, boot parameters (`rustos.boot=live` / `install`), and autoinstall format.

### glib-native

RustOS embeds **glib-native**, a `no_std` Rust port of GLib used by kernel-side GObject/GIO integration and tested on the host:

```bash
make test-glib-native    # host unit tests
make check-glib-native   # host cargo check
make build-glib-static   # static C library for userspace linking
```

## Architecture

```
┌──────────────────────────────────────────────────────────────────┐
│  Userspace (Alpine rootfs): gnome-shell, installer, busybox, …   │
├──────────────────────────────────────────────────────────────────┤
│  glib-native / GNOME bridge (glib, dbus, wayland, mutter)        │
├──────────────────────────────────────────────────────────────────┤
│  Linux compat layer (200+ APIs) + modern interfaces (io_uring,   │
│  bpf, seccomp, namespaces, cgroup, landlock, …)                  │
├──────────────────────────────────────────────────────────────────┤
│  Kernel services: scheduler, syscalls, VFS, IPC, ELF/dynlink   │
├──────────────────────────────────────────────────────────────────┤
│  Network (TCP/IP)  │  Block I/O  │  Package mgr  │  Crypto      │
├──────────────────────────────────────────────────────────────────┤
│  Driver framework: PCI, USB, VirtIO, net, storage, HID, …        │
├──────────────────────────────────────────────────────────────────┤
│  HAL: ACPI · APIC/PIC · timers · SMP · power/NUMA                │
├──────────────────────────────────────────────────────────────────┤
│  x86_64 (primary) · AArch64 target spec (experimental)           │
└──────────────────────────────────────────────────────────────────┘
```

Entry point: `entry_point!(kernel_main)` in `src/main.rs` using the `bootloader` crate. Legacy Multiboot assembly (`src/boot.s`) exists but is not linked in default builds.

## Getting started

### Prerequisites

- **Rust nightly** (`rust-toolchain.toml`) with `rust-src` and `llvm-tools-preview`
- **QEMU** (`qemu-system-x86_64`) for emulation
- **bootimage** — installed automatically by `build_rustos.sh` / `make install-deps`
- Optional for install media: `cpio`, `mksquashfs`, `xorriso`, `rsync`

### Quick setup

```bash
git clone https://github.com/spotty118/Rustos.git
cd Rustos
./build_rustos.sh --install-deps   # or: make install-deps
```

### Build & run

```bash
make build              # debug kernel (rebuilds initramfs)
make build-release      # optimized build
make bootimage          # bootable debug image
make run                # build + QEMU
make boot-smoke         # headless serial boot check
make test               # kernel #[test_case] suite
make check              # compile + link (catches linker errors)
```

Cross-architecture:

```bash
make build-x86          # x86_64-rustos.json (default)
make build-arm          # aarch64-apple-rustos.json
```

Development helpers:

```bash
make format lint        # rustfmt + clippy
make info size          # build info and binary size
make clean              # remove artifacts
```

### Expected boot (QEMU)

Serial output includes early ACPI/PCI init, memory setup, and subsystem banners. Exact messages depend on build profile and enabled features. Use `make boot-smoke` in CI or headless environments.

## Project layout

```
src/
├── main.rs              # Kernel entry, module graph, test runner
├── acpi/ apic/ pci/     # Hardware discovery
├── memory*.rs           # Physical + virtual memory
├── process/             # Lifecycle, context switch, dynamic linker
├── scheduler/ syscall/  # Scheduling and syscall dispatch (INT 0x80 + SYSCALL)
├── fs/ vfs/ initramfs/  # Filesystems and early userspace unpack
├── net/                 # TCP/IP stack
├── drivers/             # Device drivers (net, storage, usb, virtio, …)
├── gpu/ graphics/ desktop/
├── linux_compat/        # POSIX/Linux API shims
├── package/             # .deb/.rpm/.apk package manager
├── elf_loader/          # ELF binary loading
├── glib*.rs gnome/ dbus/ wayland/ mutter/  # GNOME/glib bridge
├── installer/           # Kernel-side installer hooks
└── …                    # bpf, io_uring, cgroup, trace, power, etc.

glib-rust/glib/rust/glib-native/   # no_std GLib implementation
userspace/                         # rootfs, initramfs, installer assets
tests/                             # #[test_case] integration tests
docs/                              # Architecture, build, installer, Linux app guides
scripts/                           # boot_smoke.sh, live ISO, rootfs builders
```

## Linux application support

RustOS is building toward running dynamically linked Linux binaries and `.deb` packages:

| Component | Status |
|-----------|--------|
| ELF loader | Implemented |
| x86_64 dynamic linker (relocations, symbol resolve) | Core complete; VFS `.so` loading integration ongoing |
| Linux compat API surface | Broad coverage; wiring to live VFS/network varies by call |
| Package extraction (.deb) | Implemented |
| Real distro binary smoke tests | In progress |

See [docs/LINUX_APP_SUPPORT.md](docs/LINUX_APP_SUPPORT.md), [docs/LINUX_APP_PROGRESS.md](docs/LINUX_APP_PROGRESS.md), and [docs/DYNAMIC_LINKER_INTEGRATION.md](docs/DYNAMIC_LINKER_INTEGRATION.md).

## Documentation

| Doc | Description |
|-----|-------------|
| [docs/guides/QUICKSTART.md](docs/guides/QUICKSTART.md) | Fast setup |
| [docs/BUILD_GUIDE.md](docs/BUILD_GUIDE.md) | Build, test, debug |
| [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) | System design |
| [docs/SUBSYSTEMS.md](docs/SUBSYSTEMS.md) | Subsystem reference |
| [docs/MODULE_INDEX.md](docs/MODULE_INDEX.md) | Module cross-reference |
| [docs/INSTALLER.md](docs/INSTALLER.md) | Live/install ISO pipeline |
| [docs/LINUX_COMPATIBILITY.md](docs/LINUX_COMPATIBILITY.md) | Linux API compatibility status |
| [docs/ROADMAP.md](docs/ROADMAP.md) | Development roadmap |
| [docs/SAFETY.md](docs/SAFETY.md) | `unsafe` invariants |
| [docs/FAQ.md](docs/FAQ.md) | Frequently asked questions |
| [AGENTS.md](AGENTS.md) | Contributor / agent build notes |

## Roadmap snapshot

| Area | Notes |
|------|-------|
| Core HAL & kernel services | Boot, ACPI, APIC, PCI, scheduler, syscalls — largely in place |
| Network & drivers | Stack and driver framework present; hardware validation ongoing |
| VFS + Linux app execution | APIs exist; end-to-end dynamic binary execution still being wired |
| Desktop / GNOME | Rootfs targets and kernel bridges exist; full HW desktop is experimental |
| Security hardening | seccomp, landlock, namespaces present; policy and sandboxing evolving |
| Storage & advanced VM | Block layer and swap present; demand paging and production FS work continues |

Full detail: [docs/ROADMAP.md](docs/ROADMAP.md).

## Contributing

1. Fork the repository
2. Create a branch (`cursor/<name>-150c` or your own naming)
3. Make changes with `cargo fmt` and targeted tests (`make test`, `make boot-smoke`)
4. Open a pull request with verification commands

Commit subjects: `component: Brief description` (see [docs/BUILD_GUIDE.md](docs/BUILD_GUIDE.md)).

## License

MIT License — see [LICENSE](LICENSE).

## Acknowledgments

- [Writing an OS in Rust](https://os.phil-opp.com/) (Philipp Oppermann)
- [bootloader](https://github.com/rust-osdev/bootloader) crate
- The Rust embedded and OSDev communities
- GLib/GNOME projects (upstream references in `glib-rust/`)
