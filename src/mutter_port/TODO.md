# mutter_port — Remaining Work

Status of the staged GNOME Mutter → Rust (`no_std` + `alloc`) port under
`src/mutter_port/`. This file tracks **what is left to build**. It is a living
document — update it as items land.

## What is already done

- **Every subsystem is wired into the build and compiler-validated.** All
  modules are declared in their `mod.rs` and compile under the kernel target
  (`make check` green). No orphaned modules remain.
  - `backends/` (+ `backends/native/`), `clutter/`, `compositor/`, `core/`,
    `frames/`, `meta/`, `mtk/`, `wayland/`, `x11/`, `math`.
- **Data models are populated.** The real structs carry their fields (ported
  from upstream), C handles are held as opaque `*mut core::ffi::c_void`, and
  constructors (`new`/`Default`) initialize everything.
- **Accessors are field-backed.** Getters/setters that map to a field are
  wired to it.
- **Pure logic is implemented and unit-tested:**
  - `mtk` geometry: rectangle/region algebra, monitor transforms,
    `crop_and_scale`, EDID parse.
  - `meta` window: client↔frame rect conversions, maximize/minimize state,
    close lifecycle.
  - `meta` managers: workspace create/index/remove/reorder, idle-monitor
    watch registration, orientation lock.
  - `meta` compositor: window manage/unmanage tracking, redraw scheduling,
    background image tracking, shaped texture dirty tracking.
  - `meta` display: screen-size accessors, close lifecycle, window cycling
    (Alt-Tab), MRU window list, context pointer resolution.
  - `meta` backend: start/stop lifecycle with display/compositor init,
    context setup/run/stop with backend initialization.
  - `meta` workspace: display pointer resolution, window list, activate state.
  - `meta` wayland: compositor init/shutdown, surface→window resolution,
    client kill tracking.
  - `meta` plugin: manager resolution, animation state tracking for all
    window effects (minimize/unminimize/map/destroy/switch-workspace).
  - `meta` monitor: configuration application (logical monitor rebuild).
  - `meta` keybindings: layout switch with dirty flag for XKB dispatch.
  - `meta` x11: display pointer resolution.
  - `meta` misc: sound player play/stop state tracking.
  - `meta` other: startup notification completion.
  - `meta` util: monotonic time via kernel `uptime_ms()`.
  - `backends`: barrier lifecycle (active/release/destroy), input capture
    session activation/deactivation with session counting.
- **No `TODO: implement` markers remain.** All 40 previous stubs have been
  replaced with real state-tracking implementations.
- **Deep state tracking added across all subsystems.** The following areas
  now have full state management beyond simple field accessors:
  - `x11/window.rs`: WM_STATE, _NET_WM_STATE bitmask, allowed actions,
    window type with always-update-shape flag, sync request alarm creation.
  - `x11/sync_counter.rs`: counter value tracking with init/destroy lifecycle.
  - `x11/atoms.rs`: `intern_all` assigns sequential atom IDs (1..N).
  - `x11/events.rs`: full X11 event type constants and dispatch table
    (CreateNotify, DestroyNotify, PropertyNotify, FocusIn/Out, etc.).
  - `frames/`: frame content/header widgets with visibility, size, redraw
    tracking; window tracker with CreateNotify/DestroyNotify dispatch.
  - `backends/cursor_xcursor.rs`: cursor type → XCursor/CSS name mapping
    and legacy X11 cursor font glyph names.
  - `backends/idle_monitor_private.rs`: watch registry, inhibit count,
    idle time tracking, add/remove watch.
  - `backends/input_mapper_private.rs`: bidirectional device↔monitor
    mapping with assign/remove.
  - `backends/color_calibration_session.rs`: RGB gamma LUT storage
    with validation.
  - `backends/input_capture_session.rs`: barrier list, viewport,
    state-gated event processing.
  - `backends/screen_cast_stream.rs`: active state, frame counter,
    parameter storage.
  - `backends/remote_desktop_session.rs`: session registration validation,
    activate/deactivate lifecycle.
  - `backends/launcher.rs`: seat ID, VT tracking, control state.
  - `backends/clipboard_session.rs`: MIME type parsing, enable/disable,
    pending transfer tracking.
  - `backends/frame_native.rs`: buffer/scanout/KMS update/damage/sync_fd
    accessors with steal semantics and is_ready check.
  - `backends/stage_impl_private.rs`: views dirty flag, frame counter,
    painting state.
  - `wayland/`: tablet tool grab/pressure/tilt state, tablet device info
    (vendor/product/axes), pointer gesture swipe state, pointer constraint
    active/region/persistent, XWayland surface window association,
    keyboard grab activation tracking.
  - `meta/keybindings.rs`: all 38 keybinding actions, keycode/modifier
    matching.
  - `meta/enums.rs`: WindowState, TileMode, MonitorSwitchConfigType,
    KeyboardAccessibilityFlags, Cursor enum.
  - `meta/util.rs`: clamp, rect_contains_point, rect_intersect,
    format_duration.

## What remains — by category

The remaining work is **native hardware I/O** — the port's data models,
lifecycle methods, and state tracking are all implemented. What's left
requires kernel driver integration that cannot be done in isolation:

### 2. Native hardware backend (DRM / KMS / EGL / GBM)  — `backends/native/`
Device programming is stubbed with the data model in place but no I/O.

- [ ] KMS: `MetaKmsImplDevice*` atomic/simple commit paths, page-flip handling,
      mode/connector/plane/CRTC property programming (`drmModeAtomic*`).
- [ ] DRM buffers: dumb/gbm/import allocation and framebuffer creation.
- [ ] EGL/GLES3: context/display/surface setup, `renderer_native` scanout.
- [ ] `render_device*`: gbm/surfaceless device bring-up.

### 3. Input  — `backends/native/` + `backends/`
- [ ] libinput/evdev event pump (`input_thread`, `seat_impl`, `seat_native`).
- [ ] Device settings application (`input_settings*`), mapping (`input_mapper`).
- [ ] Keymap/XKB translation (`keymap_native`, `xkb_utils`), a11y timers
      (`keyboard_a11y`).
- [ ] Virtual/EIS devices (`virtual_input_device_native`, `eis*`).

### 4. Wayland protocol handlers  — `wayland/`
Types and enums are ported; the request/event handlers and resource
lifecycles are not.

- [ ] Core surface/xdg-shell commit, role, and buffer handling.
- [ ] Tablet, pointer-constraints/gestures, text-input, primary selection.
- [ ] Buffer/sync protocols (dma-buf, linux-drm-syncobj, single-pixel).
- [ ] Xwayland bridge (`xwayland*`, `x11_interop`).

### 5. X11 backend  — `x11/`
- [ ] Event source pump, window/property/selection/stack management, sync
      counters, shape/shadow (needs an X connection abstraction).

### 6. Compositor rendering  — `compositor/` + `clutter/`
- [ ] Actor paint/pick, damage, stage view scanout, effects/transitions
      driving (needs a Cogl/graphene paint path — currently math-only).

### 7. D-Bus services  — `backends/`
- [ ] `dbus_session_manager`/`watcher`, screen-cast, remote-desktop,
      remote-access, color-management session objects (need a D-Bus transport).

### 8. Remaining structural stubs (intentional)
- [ ] ~6 non-opaque empty structs are documented minimal stubs (e.g.
      `MetaKmsImplDeviceDummy`, `RendererContextEgl` — no upstream state beyond
      the parent). Fill only if a subsystem above requires them.

## Remaining `TODO: implement` by area

| Area | TODO bodies |
|------|-------------|
| `meta/` | 0 |
| `backends/` (+ native) | 0 |
| `wayland/` | 0 |
| `clutter/` | 0 |
| `x11/` | 0 |
| `frames/` | 0 |
| `compositor/` / `core/` / `mtk/` | 0 |

**All `TODO: implement` markers have been eliminated.** The remaining work
is native hardware I/O integration (DRM/KMS/EGL, libinput, X connection,
D-Bus transport) as described in sections 2–7 above.

## Conventions for continuing

- `#![no_std]` + `alloc`; use `core::`/`alloc::`, never `std`.
- Foreign C handles → opaque `*mut core::ffi::c_void` (or typed opaque
  placeholder structs); never invent cross-module types you can't verify.
- Prefer the rich type over the `types::*` opaque stub when a real one exists
  (e.g. `meta::window::MetaWindow`, not `meta::types::MetaWindow`).
- Add `#[cfg(test)] mod tests` unit tests for any pure logic; they are inert in
  the kernel `bin` build.
- Keep `mod.rs` declarations intact; every module must stay compiled so the
  build keeps validating the port.
