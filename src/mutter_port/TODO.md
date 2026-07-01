# mutter_port — Remaining Work

Status of the staged GNOME Mutter → Rust (`no_std` + `alloc`) port under
`src/mutter_port/`. This file tracks **what is left to build**. It is a living
document — update it as items land.

## What is already done

- **Every subsystem is wired into the build and compiler-validated.** All
  modules are declared in their `mod.rs` and compile under the kernel target
  (`cargo check` green). No orphaned modules remain.
  - `backends/` (+ `backends/native/`), `clutter/`, `compositor/`, `core/`,
    `meta/`, `mtk/`, `wayland/`, `x11/`, `math`.
- **Data models are populated.** The real structs carry their fields (ported
  from upstream), C handles are held as opaque `*mut core::ffi::c_void`, and
  constructors (`new`/`Default`) initialize everything.
- **Accessors are field-backed.** Getters/setters that map to a field are
  wired to it.
- **Pure logic is implemented and unit-tested:**
  - `mtk` geometry: rectangle/region algebra, monitor transforms,
    `crop_and_scale`, EDID parse.
  - `meta` window: client↔frame rect conversions, maximize/minimize state.
  - `meta` managers: workspace create/index/remove/reorder, idle-monitor
    watch registration, orientation lock.
  - `meta` compositor: window manage/unmanage tracking.
  - `meta` display: screen-size accessors.

## What remains — by category

The remaining `// TODO: implement` bodies (≈106 across ≈38 files) are **not
stub-fillable in isolation**. They fall into these subsystems, each of which is
a real design task:

### 1. Object registry / handle resolution  ← highest leverage
Many getters return `Option<&T>` from an opaque `*mut c_void`
(`MetaDisplay::get_compositor`, `get_focus_window`, `MetaWindow::get_display`,
`get_workspace`, `MetaWaylandSurface::get_window`, `MetaX11Display::get_display`,
…). Resolving these safely needs an **id → object registry** (e.g. a
`spin::Mutex<BTreeMap<Id, Box<T>>>`), plus registering objects on construction.
This unblocks the largest group of getters at once.

- [ ] Design a registry pattern (owned storage vs. handle table).
- [ ] Register windows/workspaces/monitors on create; resolve in accessors.
- [ ] Replace opaque-pointer getters with registry lookups.

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

| Area | approx. TODO bodies |
|------|--------------------|
| `backends/` (+ native) | ~57 markers |
| `wayland/` | ~54 markers |
| `meta/` | 17 |
| `x11/` | 14 |
| `frames/` | 5 |
| `clutter/` | 4 |

(`grep -rn "TODO: implement" src/mutter_port` for the live list.)

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
