# Pending TODOs

Tracking outstanding work from the codebase audit + Linux-to-Rust port effort on
branch `claude/codebase-audit-c-shims-sftva5` (PR #79). Completed fixes are in
the PR history; this file lists what remains.

## QUIC (`src/net/quic/`)

Mirrors the in-kernel Linux QUIC module (github.com/lxin/quic).

Done (each validated against RFC test vectors where applicable):
- [x] **Crypto primitives** — AES-CTR, AES-128/256-GCM (constant-time GHASH),
      HMAC-SHA256, HKDF + HKDF-Expand-Label (`crypto/gcm.rs`, `crypto/hkdf.rs`;
      NIST/McGrew + RFC 4231/5869 vectors).
- [x] **Key derivation** (`keys.rs`) — Initial keys from DCID + v1 salt and
      "quic key/iv/hp" from a traffic secret (RFC 9001 A.1 vectors).
- [x] **Packet protection** (`protection.rs`) — AEAD seal/open (nonce = IV XOR
      PN, header as AAD) + header-protection mask (RFC 9001 A.2 vector).
- [x] **Packet I/O** (`io.rs`) — build/open protected short-header packets and
      apply received frames (round-trip + tamper tests).
- [x] **UDP glue** (`udp.rs`) — QUIC port registry; UDP receive (post-checksum)
      routes by DCID to an endpoint, unprotects, and applies frames for 1-RTT
      connections, and accepts new server-side Initials (derive keys, decrypt,
      buffer CRYPTO).
- [x] **Long-header I/O** (`io.rs`) — build/open Initial & Handshake packets
      (token/length parsing, Length-bounded AEAD; RFC 9001 A.1 validated).
- [x] **Send path + ACK** (`send.rs`, `pnspace.rs`) — coalesced received-range
      tracking → ACK frames; packetize ACK + STREAM data under the congestion
      window into protected packets.
- [x] **Handshake secret install** (`connection.rs`) — `install_initial_keys`
      from the DCID and `install_secret` for per-level traffic secrets.

Remaining:
- [ ] **TLS handshake driver** — feed reassembled CRYPTO to the userspace TLS
      side and pump its output back as CRYPTO frames until the handshake
      completes and 1-RTT secrets are installed (the offload interface).
- [ ] **Loss recovery** — arm PTO from `timer`, detect loss, and retransmit
      (ACK generation + cwnd are wired; the retransmit timer loop is not).
- [ ] **Stream reassembly** — out-of-order STREAM data buffering (in-order
      delivery is wired today).
- [ ] **Connection ID management** — issue/retire NEW_CONNECTION_ID, stateless
      reset tokens; migrate the server connection key from the client DCID to
      the server-issued SCID after the handshake.
- [ ] (optional) **ChaCha20-Poly1305** as the alternate cipher suite.

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
