# Boot-to-Desktop, in Rust — Stage Map

**Target (set 2026-06-30):** reproduce Linux's *complete* boot-to-desktop sequence —
PID 1 → device manager → D-Bus → seat/login → display manager → Wayland
compositor/shell — **as native Rust services**, faithful to the real Linux flow
and ordering, but Rust code. **No external ELF binaries, no musl, no dynamic
linker.** This deliberately replaces the previous "run unmodified upstream GNOME
binaries via linux-compat" plan.

Reframe from the earlier "I don't want the kernel desktop": under *this* target,
the in-kernel Rust compositor/shell **is** the deliverable. The objection before
was to a fake look-alike *instead of* real GNOME. Now the explicit goal is a
faithful Rust reimplementation of the Linux desktop boot, so the existing native
stack is an **asset to build on**, not the rejected route.

---

## Existing native-Rust foundation (reuse — do not rewrite)

| Component        | File(s)                          | State |
|------------------|----------------------------------|-------|
| Wayland compositor (wire protocol, surfaces, shm, output, **wl_seat** input, render) | `src/wayland/{mod,server,core_protocol,input,render}.rs` | Substantial; real-client AF_UNIX transport verified (commit 8a90497) |
| D-Bus message bus (signature marshalling, GNOME IPC) | `src/dbus/mod.rs` | Substantial, in-kernel |
| Session runtime layout (`XDG_RUNTIME_DIR`, `/run/user/0/bus`, `/run/user/0/wayland-0`) | `src/gnome_overlay.rs` | Present |
| GLib-equivalent primitives | `src/glib.rs` | Present |
| Desktop main loops (compositor idle / render) | `src/main.rs` (`modern_desktop_main_loop`, `userspace_session_loop`) | Present |
| Linux syscall ABI layer | `src/linux_compat/*` | Substantial (kept for in-process services / future) |

> Note: `wl_seat` in `src/wayland` is the compositor's **input abstraction** — it
> is NOT `seat0`. seat0 (hardware-seat/session management) is a separate, missing
> thing (see Stage 5).

---

## The real Linux boot-to-GNOME(Wayland) sequence, mapped

Canonical modern path: kernel → systemd (PID 1) → systemd-udevd → D-Bus system
bus → systemd-logind → gdm → user session (`systemd --user` → session D-Bus →
**gnome-shell**). Each row = one Rust stage to build, in order.

| # | Linux stage | What it really does | Rust target | RustOS state |
|---|-------------|---------------------|-------------|--------------|
| 0 | **(prereq) stable base** | — | Isolated, non-racy checkout so boots are reproducible | **BLOCKER** — tree has 105 uncommitted files; other agents mutate scheduler/process/desktop mid-build. Isolate before implementing. |
| 1 | **PID 1 / init** | mount `/proc /sys /dev /run`, set up `/dev/console` + fd 0/1/2, own the service graph, reap orphans, reach `default.target` | A real Rust **service/unit manager** (ordering, deps, targets, restart). Replaces the current "exec one init ELF" / marker-file shortcut. | **PARTIAL → MISSING**. `src/linux_compat/desktop.rs` only creates `/run` dirs and writes `graphical.target` *marker files*; there is **no unit/service manager**. |
| 2 | **systemd-udevd / devtmpfs** | populate `/dev` from the device model, coldplug existing devices, deliver uevents | A Rust **device-manager service**: walk the kernel device model, create `/dev` nodes in VFS, emit uevents to later stages | **MISSING as a service** (kernel has device model + drivers, but nothing populates `/dev` / emits uevents in the boot graph) |
| 3 | **D-Bus system bus** | `dbus-daemon --system` at `/run/dbus/system_bus_socket`; name ownership, routing, policy | Wire `src/dbus/mod.rs` as the **system bus** behind that socket path, started as a Stage-1 unit | **PARTIAL** — bus engine exists; needs to be *started as a service* and exposed at the system socket (session bus at `/run/user/0/bus` already set up by `gnome_overlay`) |
| 4 | **systemd-logind** | seat/session tracker + **device-access broker**: `CreateSession`, `TakeControl`, `TakeDevice` hand DRM/input fds to the compositor over D-Bus. **Not** DRM master itself. | A Rust **logind service** on the system bus: model `seat0`, sessions, and broker DRM/input fd access to the compositor | **MISSING** |
| 5 | **seat0 (session/seat mgmt)** | the hardware seat logind manages (distinct from wl_seat) | Seat/session objects owned by the Stage-4 logind service | **MISSING** (don't confuse with the existing `wl_seat`) |
| 6 | **gdm (display manager)** | start a Wayland **greeter** (itself a gnome-shell instance), authenticate, then launch the user session | A Rust **display-manager service**: open a greeter compositor session, then hand off to the user session. (Auth can be trivial/auto-login first.) | **MISSING** |
| 7 | **user session: `systemd --user` + session D-Bus + gnome-shell** | `systemd --user` starts the session bus and `org.gnome.Shell`. **gnome-shell links libmutter** — the *shell process is the Wayland compositor*. There is **no separate mutter process**. | The existing `src/wayland` compositor **is** this stage. Drive it as the session's compositor-shell unit; the compositor becomes **DRM master** (using fds brokered by Stage 4). Add panel/overview/launcher as shell features. | **PARTIAL** — compositor exists; needs to run *as the session's shell-compositor unit* and become DRM master via logind, not be jumped-into directly. |
| 8 | **gnome-settings-daemon & friends** | autostart D-Bus services (settings, keyring, notifications, xdg portals) | Rust services autostarted by the session manager over the session bus | **MISSING / later** |

### Stage-collapse corrections (do NOT draw these as separate boxes)
- **mutter + gnome-shell = ONE process/stage** (Stage 7). gnome-shell links
  libmutter; the shell *is* the compositor.
- The **gdm greeter is also a gnome-shell instance** (a compositor session), not
  a distinct rendering technology.
- **logind ≠ DRM master.** logind brokers device fds; the **compositor** is DRM
  master.

---

## Critical path to first pixels (achievable early milestone)

Distinct from full fidelity. Ship this first, then layer the rest:

> **Stage 1 (minimal init/service-manager) → Stage 7 (compositor-shell drawing
> ONE surface on the framebuffer).**

Skip udev/logind/gdm/gsd for the milestone: init starts the compositor-shell
directly, compositor takes the framebuffer directly (DRM-master brokering deferred
to when logind exists). Goal: a real boot → service-manager → compositor renders a
window. Then insert Stages 2–6 underneath without changing the endpoints.

After the milestone, fidelity order: **2 (udev/dev) → 3 (system bus as service) →
4+5 (logind/seat0 + fd brokering) → 6 (greeter/gdm) → 8 (gsd/portals)**.

---

## Implementation prerequisites (before writing stage code)

1. **Isolate the base (Stage 0).** Create a git worktree off the current
   feature-branch HEAD so concurrent automation can't mutate source mid-build.
   Last session's non-reproducible hangs came from the racy tree, not from the
   code under test. **This blocks implementation, not this map.**
2. Respect the **12-core cap** (`CARGO_BUILD_JOBS=12`); one QEMU at a time.
3. Boots are ~90s on TCG; init/service-manager runs *after*
   `[kernel] system_state = RUNNING` (see `src/main.rs:1812`).

---

## Open design questions (resolve at implementation time, not now)

- **Where do the services run?** In-kernel Rust tasks/threads, or as separate
  address-space processes driven via `linux_compat`? The new target says "no
  external binaries" — leaning **in-kernel Rust services** communicating over the
  existing AF_UNIX/D-Bus transports. Confirm before Stage 1.
- **Unit model fidelity:** how close to systemd unit semantics (deps, ordering,
  `Wants`/`Requires`, targets) vs. a hardcoded boot graph for the milestone.
- **Auth at the greeter:** auto-login first; real PAM-equivalent later.
