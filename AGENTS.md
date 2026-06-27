# Repository Guidelines

## Project Structure & Module Organization
- `src/main.rs`: bootloader entrypoint (`bootloader` crate) for x86_64; `src/boot.s` + `linker.ld` are the legacy multiboot path.
- `src/`: kernel subsystems live under `src/memory_basic.rs`, `src/interrupts.rs`, `src/gdt.rs`, `src/drivers/`, `src/net/`, `src/fs/`, `src/process/`, `src/linux_compat/`, `src/desktop/`, and `src/graphics/`.
- `tests/`: `#![no_std]` test binaries using the custom test runner (`#[test_case]`).
- `docs/`: build guides, architecture notes, safety and debugging docs.
- `scripts/` and `build_rustos.sh`: build/run automation; `scripts/boot_smoke.sh` is the headless QEMU boot check.
- `userspace/`: initramfs/rootfs assets; `experimental/` holds standalone/multiboot experiments.
- Target specs and linker scripts live at the repo root (`x86_64-rustos.json`, `aarch64-apple-rustos.json`, `linker.ld`, `link.ld`).

## Build, Test, and Development Commands
- `make build` / `make build-release`: compile the kernel (debug/release).
- `make bootimage` / `make run`: create a bootable image and run in QEMU.
- `make boot-smoke`: headless QEMU run that checks for the boot banner on serial.
- `make test` / `make test-glib-native`: kernel tests vs glib-native host unit tests.
- `make check`: fast compile check without building artifacts.
- `./build_rustos.sh --check-only --test --release`: scripted builds; see `./build_rustos.sh --help`.
- `RUSTOS_QEMU_DISPLAY=cocoa|gtk` selects the QEMU display backend for scripts.

## Coding Style & Naming Conventions
- Rust nightly is required (`rust-toolchain.toml`); the kernel is `no_std`, so prefer `core`/`alloc` over `std`.
- Format with `cargo fmt`; lint with `cargo clippy --target x86_64-rustos.json -Zbuild-std=core,alloc,compiler_builtins`.
- Naming: `snake_case` for modules/functions/files, `CamelCase` for types/traits, `SCREAMING_SNAKE_CASE` for constants.
- Document `unsafe` invariants in `docs/SAFETY.md` and add inline SAFETY comments.

## Testing Guidelines
- Use module tests under `#[cfg(test)]` and custom `#[test_case]` tests in `tests/`.
- Targeted runs: `cargo test --target x86_64-rustos.json -- <test-name>` when filtering the custom runner.
- For boot validation after kernel changes, run `make boot-smoke` or `make run` in QEMU.

## Commit & Pull Request Guidelines
- Commit format (from `docs/BUILD_GUIDE.md`):
  ```
  component: Brief description

  Longer explanation if needed.
  Fixes: #issue-number
  ```
- Git history is not included in this checkout; follow the format above and keep subjects imperative.
- PRs: include a short problem/solution summary, verification commands, and relevant docs updates.

## Agent-Specific Notes
- If using automated tools, align with `CLAUDE.md` for build/test expectations and architecture context.
