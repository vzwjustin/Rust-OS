# Pending TODOs

Tracking outstanding work from the codebase audit + Linux-to-Rust port effort on
branch `claude/codebase-audit-c-shims-sftva5` (PR #79). Completed fixes are in
the PR history; this file lists what remains.

## QUIC (`src/net/quic/`)

Mirrors the in-kernel Linux QUIC module (github.com/lxin/quic). Foundation and
endpoint demux are done; the data path is not yet wired.

- [ ] **AEAD packet protection (RFC 9001)** — blocked on crypto primitives.
      `crypto/aes.rs` has only block + CBC today; QUIC needs:
  - [ ] AES-CTR (straightforward on top of `encrypt_block`) for header
        protection.
  - [ ] AES-128-GCM (needs a GHASH / carryless-multiply implementation) for
        packet payload AEAD.
  - [ ] (optional) ChaCha20-Poly1305 as the alternate cipher suite.
- [ ] **UDP socket glue** — register the QUIC family in `net/socket.rs` and call
      `quic::endpoint::QuicEndpoint::route()` from the UDP receive path so
      inbound datagrams reach connections.
- [ ] **Receive path** — once AEAD lands: remove header/packet protection,
      decode the packet number against the PN space, walk frames, and apply
      them to the matched `Connection` (CRYPTO → handshake, STREAM → streams,
      ACK → loss recovery, etc.).
- [ ] **Send path** — packetize stream/crypto data under congestion control,
      assign packet numbers, protect, and emit.
- [ ] **ACK generation + loss recovery scheduling** — wire `pnspace`, `timer`
      (PTO), and `cong` together into a real send/ack/retransmit loop.
- [ ] **Userspace handshake hand-off** — interface to receive negotiated TLS 1.3
      traffic secrets per encryption level (handshake is offloaded, as upstream).
- [ ] **Connection ID management** — issue/retire NEW_CONNECTION_ID, stateless
      reset tokens.

## Audit follow-ups (deferred, not safety bugs)

- [ ] **AHCI hardcodes port 0** (`drivers/storage/ahci.rs`) — `execute_command`
      always targets port 0, so a disk on any other port is unusable. Needs a
      device→port mapping plumbed through read/write.
- [ ] **Hotplug frame accounting** (`memory.rs` `remove_usable_range`) — the
      frame count it subtracts uses a different basis than `mm.total_memory`, so
      a partial offline drifts the two counters (stats only, no corruption).
- [ ] **Futex PI requeue** (`linux_compat/thread_ops.rs`) —
      `FUTEX_WAIT_REQUEUE_PI` / `FUTEX_CMP_REQUEUE_PI` still return ENOSYS.

## C decompressors (`c_libs/`)

Now memory-safe against crafted input, but functionally incomplete vs. the real
formats. The right long-term direction (matching the "mirror Linux in Rust"
goal) is Rust reimplementations rather than further C patching.

- [ ] **xz/LZMA2**: the chunk-control framing diverges from the real format; the
      `lit` / `pos_decoders` probability tables are undersized for standard
      `lc`/`lp` (currently rejected rather than decoded — see the guards).
- [ ] **bzip2**: RLE1 stage, Huffman base/limit conventions, and MTF-decoding of
      selectors are incomplete, so output is wrong for many real payloads.
- [ ] Consider replacing all three with pure-Rust decoders (zstd/xz/bzip2) or
      gating the package manager on the formats that work.

## Broader Linux-to-Rust wiring

- [ ] **Subsystem self-registration** — several driver `init()`s print
      "subsystem ready" without registering a software device (mmc, mtd, ufs,
      acpi, ntb, cdx). They appear to wait for hardware probe; decide whether
      each should register a default software device (as pwm/gpio/nfc/gnss/
      hwspinlock/edac now do).
- [ ] **Dead-code subsystems** — many modules are implemented but never wired
      into the boot/dispatch path (the bulk of the `never used` warnings). These
      are the natural next units of the port: wire each into init/syscall
      dispatch as it is completed.

## Notes

- `context_switch_asm` and `clone_table` were investigated and intentionally
  left unchanged: the former is correct for its kernel↔kernel use (user entry
  uses the separate `switch_to_user_mode` iretq path), and the latter has no
  fork caller (unused, already documented). Revisit `clone_table` only when
  implementing COW fork.
