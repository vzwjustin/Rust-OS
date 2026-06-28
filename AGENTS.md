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
- `make build` / `make build-release`: compile and link the kernel (debug/release).
- `make bootimage` / `make run`: create a bootable image and run in QEMU.
- `make boot-smoke`: headless QEMU run that checks for the boot banner on serial.
- `make test` / `make test-glib-native`: kernel tests vs glib-native host unit tests.
- `make check`: compile and link the debug kernel (same as `make build`; catches link failures that `cargo check` / `--check-only` miss).
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
- **Code reading**: When reading files for reasoning about stubs or code changes, use 30-40 lines of context above and below the target area. Wider context windows keep the agent grounded and make RTK's compressed output more useful.


<!-- headroom:rtk-instructions -->
# RTK (Rust Token Killer) - Token-Optimized Commands

When running shell commands, **always prefix with `rtk`**. This reduces context
usage by 60-90% with zero behavior change. If rtk has no filter for a command,
it passes through unchanged — so it is always safe to use.

## Key Commands
```bash
# Git (59-80% savings)
rtk git status          rtk git diff            rtk git log

# Files & Search (60-75% savings)
rtk ls <path>           rtk read <file>         rtk grep <pattern>
rtk find <pattern>      rtk diff <file>

# Test (90-99% savings) — shows failures only
rtk pytest tests/       rtk cargo test          rtk test <cmd>

# Build & Lint (80-90% savings) — shows errors only
rtk tsc                 rtk lint                rtk cargo build
rtk prettier --check    rtk mypy                rtk ruff check

# Analysis (70-90% savings)
rtk err <cmd>           rtk log <file>          rtk json <file>
rtk summary <cmd>       rtk deps                rtk env

# GitHub (26-87% savings)
rtk gh pr view <n>      rtk gh run list         rtk gh issue list

# Infrastructure (85% savings)
rtk docker ps           rtk kubectl get         rtk docker logs <c>

# Package managers (70-90% savings)
rtk pip list            rtk pnpm install        rtk npm run <script>
```

## Rules
- In command chains, prefix each segment: `rtk git add . && rtk git commit -m "msg"`
- For debugging, use raw command without rtk prefix
- `rtk proxy <cmd>` runs command without filtering but tracks usage
<!-- /headroom:rtk-instructions -->

@RTK.md
