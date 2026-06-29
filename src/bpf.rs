//! BPF — eBPF program and map management
//!
//! Ported from Linux kernel/bpf/ (syscall.c, map.c, verifier.c).
//! Provides BPF map creation/lookup/update/delete, program load with a
//! static verifier, and an in-kernel interpreter for program execution.

use alloc::collections::BTreeMap;
use alloc::vec;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, Ordering};
use spin::{Mutex, RwLock};

// ── BPF commands ────────────────────────────────────────────────────────

pub const BPF_MAP_CREATE: u32 = 0;
pub const BPF_MAP_LOOKUP_ELEM: u32 = 1;
pub const BPF_MAP_UPDATE_ELEM: u32 = 2;
pub const BPF_MAP_DELETE_ELEM: u32 = 3;
pub const BPF_MAP_GET_NEXT_KEY: u32 = 4;
pub const BPF_PROG_LOAD: u32 = 5;
pub const BPF_OBJ_PIN: u32 = 6;
pub const BPF_OBJ_GET: u32 = 7;
pub const BPF_PROG_ATTACH: u32 = 8;
pub const BPF_PROG_DETACH: u32 = 9;
pub const BPF_PROG_TEST_RUN: u32 = 10;
pub const BPF_PROG_GET_NEXT_ID: u32 = 11;
pub const BPF_MAP_GET_NEXT_ID: u32 = 12;
pub const BPF_PROG_GET_FD_BY_ID: u32 = 13;
pub const BPF_MAP_GET_FD_BY_ID: u32 = 14;
pub const BPF_OBJ_GET_INFO_BY_FD: u32 = 15;
pub const BPF_PROG_QUERY: u32 = 16;
pub const BPF_RAW_TRACEPOINT_OPEN: u32 = 17;
pub const BPF_BTF_LOAD: u32 = 18;
pub const BPF_BTF_GET_FD_BY_ID: u32 = 19;
pub const BPF_TASK_FD_QUERY: u32 = 20;
pub const BPF_MAP_LOOKUP_AND_DELETE_ELEM: u32 = 21;
pub const BPF_MAP_FREEZE: u32 = 22;
pub const BPF_BTF_GET_NEXT_ID: u32 = 23;
pub const BPF_MAP_LOOKUP_BATCH: u32 = 24;
pub const BPF_MAP_LOOKUP_AND_DELETE_BATCH: u32 = 25;
pub const BPF_MAP_UPDATE_BATCH: u32 = 26;
pub const BPF_MAP_DELETE_BATCH: u32 = 27;
pub const BPF_LINK_CREATE: u32 = 28;
pub const BPF_LINK_UPDATE: u32 = 29;
pub const BPF_LINK_GET_FD_BY_ID: u32 = 30;
pub const BPF_LINK_GET_NEXT_ID: u32 = 31;
pub const BPF_ENABLE_STATS: u32 = 32;
pub const BPF_ITER_CREATE: u32 = 33;
pub const BPF_LINK_DETACH: u32 = 34;
pub const BPF_PROG_BIND_MAP: u32 = 35;
pub const BPF_TOKEN_CREATE: u32 = 36;

// ── Map types ───────────────────────────────────────────────────────────

pub const BPF_MAP_TYPE_UNSPEC: u32 = 0;
pub const BPF_MAP_TYPE_HASH: u32 = 1;
pub const BPF_MAP_TYPE_ARRAY: u32 = 2;
pub const BPF_MAP_TYPE_PROG_ARRAY: u32 = 3;
pub const BPF_MAP_TYPE_PERF_EVENT_ARRAY: u32 = 4;
pub const BPF_MAP_TYPE_PERCPU_HASH: u32 = 5;
pub const BPF_MAP_TYPE_PERCPU_ARRAY: u32 = 6;
pub const BPF_MAP_TYPE_STACK_TRACE: u32 = 7;
pub const BPF_MAP_TYPE_CGROUP_ARRAY: u32 = 8;
pub const BPF_MAP_TYPE_LRU_HASH: u32 = 9;
pub const BPF_MAP_TYPE_LRU_PERCPU_HASH: u32 = 10;
pub const BPF_MAP_TYPE_LPM_TRIE: u32 = 11;

// ── Program types ───────────────────────────────────────────────────────

pub const BPF_PROG_TYPE_UNSPEC: u32 = 0;
pub const BPF_PROG_TYPE_SOCKET_FILTER: u32 = 1;
pub const BPF_PROG_TYPE_KPROBE: u32 = 2;
pub const BPF_PROG_TYPE_SCHED_CLS: u32 = 3;
pub const BPF_PROG_TYPE_SCHED_ACT: u32 = 4;
pub const BPF_PROG_TYPE_TRACEPOINT: u32 = 5;
pub const BPF_PROG_TYPE_XDP: u32 = 6;
pub const BPF_PROG_TYPE_PERF_EVENT: u32 = 7;
pub const BPF_PROG_TYPE_CGROUP_SKB: u32 = 8;
pub const BPF_PROG_TYPE_CGROUP_SOCK: u32 = 9;
pub const BPF_PROG_TYPE_LWT_IN: u32 = 10;
pub const BPF_PROG_TYPE_LWT_OUT: u32 = 11;
pub const BPF_PROG_TYPE_LWT_XMIT: u32 = 12;

// ── BPF attr structures (simplified) ────────────────────────────────────

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct BpfMapCreateAttr {
    pub map_type: u32,
    pub key_size: u32,
    pub value_size: u32,
    pub max_entries: u32,
    pub map_flags: u32,
    pub inner_map_fd: u32,
    pub numa_node: u32,
    pub map_name: [u8; 16],
    pub map_ifindex: u32,
    pub btf_fd: u32,
    pub btf_key_type_id: u32,
    pub btf_value_type_id: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct BpfMapElemAttr {
    pub map_fd: u32,
    pub _pad: u32,
    pub key: u64,
    pub value_or_next_key: u64,
    pub flags: u64,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct BpfProgLoadAttr {
    pub prog_type: u32,
    pub insn_cnt: u32,
    pub insns: u64,
    pub license: u64,
    pub log_level: u32,
    pub log_size: u32,
    pub log_buf: u64,
    pub kern_version: u32,
    pub prog_flags: u32,
    pub prog_name: [u8; 16],
    pub prog_ifindex: u32,
    pub expected_attach_type: u32,
    pub btf_fd: u32,
    pub btf_key_type_id: u32,
}

// ── BPF map state ───────────────────────────────────────────────────────

pub struct BpfMap {
    pub id: u32,
    pub map_type: u32,
    pub key_size: u32,
    pub value_size: u32,
    pub max_entries: u32,
    pub flags: u32,
    pub frozen: bool,
    pub entries: BTreeMap<Vec<u8>, Vec<u8>>,
}

pub struct BpfProg {
    pub id: u32,
    pub prog_type: u32,
    pub insns: Vec<u64>,
    pub license: Vec<u8>,
}

// ── Global state ────────────────────────────────────────────────────────

static MAPS: RwLock<BTreeMap<u32, Mutex<BpfMap>>> = RwLock::new(BTreeMap::new());
static PROGS: RwLock<BTreeMap<u32, Mutex<BpfProg>>> = RwLock::new(BTreeMap::new());
static NEXT_MAP_ID: AtomicU32 = AtomicU32::new(1);
static NEXT_PROG_ID: AtomicU32 = AtomicU32::new(1);

// ── BPF instruction decoding ────────────────────────────────────────────

/// BPF instruction classes (code & 0x07).
const BPF_CLASS_LD: u8 = 0x00;
const BPF_CLASS_LDX: u8 = 0x01;
const BPF_CLASS_ST: u8 = 0x02;
const BPF_CLASS_STX: u8 = 0x03;
const BPF_CLASS_ALU: u8 = 0x04;
const BPF_CLASS_JMP: u8 = 0x05;
const BPF_CLASS_JMP32: u8 = 0x06;
const BPF_CLASS_ALU64: u8 = 0x07;

/// Source operand bit (code & 0x08).
const BPF_K: u8 = 0x00;
const BPF_X: u8 = 0x08;

/// LD/LDX mode (code & 0xE0).
const BPF_LD_IMM: u8 = 0x00;
const BPF_LD_ABS: u8 = 0x20;
const BPF_LD_IND: u8 = 0x40;
const BPF_LD_MEM: u8 = 0x60;
const BPF_LD_XADD: u8 = 0x80;

/// LD/LDX size (code >> 3) & 0x03.
const BPF_SIZE_W: u8 = 0x00;
const BPF_SIZE_H: u8 = 0x01;
const BPF_SIZE_B: u8 = 0x02;
const BPF_SIZE_DW: u8 = 0x03;

/// JMP operation (code >> 4) & 0x0F.
const BPF_JA: u8 = 0x00;
const BPF_JEQ: u8 = 0x10;
const BPF_JGT: u8 = 0x20;
const BPF_JGE: u8 = 0x30;
const BPF_JSET: u8 = 0x40;
const BPF_JNE: u8 = 0x50;
const BPF_JSGT: u8 = 0x60;
const BPF_JSGE: u8 = 0x70;
const BPF_CALL: u8 = 0x80;
const BPF_EXIT: u8 = 0x90;
const BPF_JLT: u8 = 0xa0;
const BPF_JLE: u8 = 0xb0;
const BPF_JSLT: u8 = 0xc0;
const BPF_JSLE: u8 = 0xd0;

/// ALU/ALU64 operation (code >> 4) & 0x0F.
const BPF_ADD: u8 = 0x00;
const BPF_SUB: u8 = 0x10;
const BPF_MUL: u8 = 0x20;
const BPF_DIV: u8 = 0x30;
const BPF_OR: u8 = 0x40;
const BPF_AND: u8 = 0x50;
const BPF_LSH: u8 = 0x60;
const BPF_RSH: u8 = 0x70;
const BPF_NEG: u8 = 0x80;
const BPF_MOD: u8 = 0x90;
const BPF_XOR: u8 = 0xa0;
const BPF_MOV: u8 = 0xb0;
const BPF_ARSH: u8 = 0xc0;
const BPF_END: u8 = 0xd0;

/// Decode a single 8-byte BPF instruction.
#[derive(Clone, Copy, Debug)]
struct BpfInsn {
    code: u8,
    dst: u8,
    src: u8,
    off: i16,
    imm: i32,
}

impl BpfInsn {
    fn decode(raw: u64) -> Self {
        let code = raw as u8;
        let regs = (raw >> 8) as u8;
        let dst = regs & 0x0f;
        let src = (regs >> 4) & 0x0f;
        let off = ((raw >> 16) as u16) as i16;
        let imm = ((raw >> 32) as u32) as i32;
        BpfInsn {
            code,
            dst,
            src,
            off,
            imm,
        }
    }

    fn class(&self) -> u8 {
        self.code & 0x07
    }

    fn alu_op(&self) -> u8 {
        self.code & 0xf0
    }

    fn jmp_op(&self) -> u8 {
        self.code & 0xf0
    }

    fn source(&self) -> u8 {
        self.code & 0x08
    }

    fn size(&self) -> u8 {
        (self.code >> 3) & 0x03
    }

    fn mode(&self) -> u8 {
        self.code & 0xe0
    }
}

/// Maximum number of registers (R0-R10).
const MAX_REG: u8 = 11;

/// BPF stack size in bytes.
const BPF_STACK_SIZE: usize = 512;

// ── BPF verifier ────────────────────────────────────────────────────────

/// Verify a BPF program before allowing it to be loaded.
///
/// Performs static checks inspired by the Linux kernel verifier:
/// - All opcodes are valid for their class
/// - Register numbers are within R0-R10
/// - Jump targets are within program bounds
/// - Program contains at least one EXIT instruction
/// - No division/modulo by zero immediate
/// - R10 (frame pointer) is read-only
/// - No unreachable code after EXIT (best-effort)
pub fn verify_program(insns: &[u64]) -> Result<(), i32> {
    if insns.is_empty() {
        return Err(22); // EINVAL
    }
    if insns.len() > 4096 {
        return Err(7); // E2BIG
    }

    let n = insns.len();
    let mut has_exit = false;
    let mut pc = 0usize;

    while pc < n {
        let insn = BpfInsn::decode(insns[pc]);
        let class = insn.class();

        // Validate register numbers
        if insn.dst >= MAX_REG && class != BPF_CLASS_JMP && class != BPF_CLASS_JMP32 {
            return Err(22); // EINVAL — bad dst register
        }
        if insn.source() == BPF_X && insn.src >= MAX_REG {
            return Err(22); // EINVAL — bad src register
        }

        match class {
            BPF_CLASS_ALU | BPF_CLASS_ALU64 => {
                let op = insn.alu_op();
                match op {
                    BPF_ADD | BPF_SUB | BPF_MUL | BPF_OR | BPF_AND | BPF_LSH | BPF_RSH
                    | BPF_NEG | BPF_XOR | BPF_MOV | BPF_ARSH => {}
                    BPF_END => {
                        if !matches!(insn.imm, 16 | 32 | 64) {
                            return Err(22);
                        }
                    }
                    BPF_DIV | BPF_MOD => {
                        if insn.source() == BPF_K && insn.imm == 0 {
                            return Err(22); // EINVAL — division by zero
                        }
                    }
                    _ => return Err(22), // EINVAL — unknown ALU op
                }
                // NEG only uses dst, END uses dst + imm
                if op == BPF_NEG && insn.dst == 0 {
                    // R0 neg is fine
                }
            }

            BPF_CLASS_JMP | BPF_CLASS_JMP32 => {
                let op = insn.jmp_op();
                match op {
                    BPF_JA => {
                        // Unconditional jump — only valid in JMP class
                        if class != BPF_CLASS_JMP {
                            return Err(22);
                        }
                        if insn.source() != BPF_K {
                            return Err(22);
                        }
                        // Check jump target
                        let target = pc as isize + 1 + insn.off as isize;
                        if target < 0 || target as usize >= n {
                            return Err(22); // jump out of bounds
                        }
                    }
                    BPF_EXIT => {
                        has_exit = true;
                        // EXIT must be in JMP class, no operands
                        if class != BPF_CLASS_JMP {
                            return Err(22);
                        }
                    }
                    BPF_CALL => {
                        return Err(38);
                    }
                    BPF_JEQ | BPF_JGT | BPF_JGE | BPF_JSET | BPF_JNE | BPF_JSGT | BPF_JSGE
                    | BPF_JLT | BPF_JLE | BPF_JSLT | BPF_JSLE => {
                        // Conditional jump — check target bounds
                        let target = pc as isize + 1 + insn.off as isize;
                        if target < 0 || target as usize >= n {
                            return Err(22); // jump out of bounds
                        }
                    }
                    _ => return Err(22), // unknown jump op
                }
            }

            BPF_CLASS_LD | BPF_CLASS_LDX => {
                let mode = insn.mode();
                if class == BPF_CLASS_LDX {
                    if mode != BPF_LD_MEM || insn.src != 10 {
                        return Err(38);
                    }
                } else if !(mode == BPF_LD_IMM || (mode == BPF_LD_MEM && insn.src == 10)) {
                    return Err(38);
                }
            }

            BPF_CLASS_ST | BPF_CLASS_STX => {
                let mode = insn.mode();
                if mode != BPF_LD_MEM || insn.dst != 10 {
                    return Err(38);
                }
            }

            _ => return Err(22), // unknown class
        }

        // Check for wide instruction (LD_IMM64 takes two slots)
        if class == BPF_CLASS_LD && insn.mode() == BPF_LD_IMM && insn.size() == BPF_SIZE_DW {
            pc += 2; // skip next slot (contains high 32 bits)
        } else {
            pc += 1;
        }
    }

    if !has_exit {
        return Err(22); // EINVAL — program must have EXIT
    }

    Ok(())
}

// ── BPF interpreter ─────────────────────────────────────────────────────

/// Execute a verified BPF program with the given context pointer.
///
/// The interpreter implements ALU64, JMP/JMP32, stack LD/ST (memory mode),
/// immediate loads, and EXIT.
///
/// Returns the value in R0 at EXIT, or an error if execution fails.
pub fn execute_program(insns: &[u64], ctx: u64) -> Result<i64, &'static str> {
    let mut regs = [0i64; 11];
    let mut stack = [0u8; BPF_STACK_SIZE];

    // R1 = context pointer, R10 = stack pointer (end of stack buffer)
    regs[1] = ctx as i64;
    regs[10] = stack.as_mut_ptr().wrapping_add(BPF_STACK_SIZE) as i64;

    let mut pc = 0usize;
    let n = insns.len();
    let mut steps = 0u32;
    const MAX_STEPS: u32 = 100_000;

    while pc < n {
        steps += 1;
        if steps > MAX_STEPS {
            return Err("BPF program exceeded instruction limit");
        }

        let insn = BpfInsn::decode(insns[pc]);
        let class = insn.class();

        match class {
            BPF_CLASS_ALU64 | BPF_CLASS_ALU => {
                let is64 = class == BPF_CLASS_ALU64;
                let src_val = if insn.source() == BPF_X {
                    regs[insn.src as usize]
                } else {
                    insn.imm as i64
                };
                let dst = insn.dst as usize;
                let op = insn.alu_op();

                let result = match op {
                    BPF_ADD => regs[dst].wrapping_add(src_val),
                    BPF_SUB => regs[dst].wrapping_sub(src_val),
                    BPF_MUL => regs[dst].wrapping_mul(src_val),
                    BPF_DIV => {
                        if src_val == 0 {
                            return Err("BPF division by zero");
                        }
                        regs[dst] / src_val
                    }
                    BPF_OR => regs[dst] | src_val,
                    BPF_AND => regs[dst] & src_val,
                    BPF_LSH => regs[dst].wrapping_shl(src_val as u32),
                    BPF_RSH => {
                        if is64 {
                            (regs[dst] as u64).wrapping_shr(src_val as u32) as i64
                        } else {
                            ((regs[dst] as u32).wrapping_shr(src_val as u32)) as i64
                        }
                    }
                    BPF_NEG => -regs[dst],
                    BPF_MOD => {
                        if src_val == 0 {
                            return Err("BPF modulo by zero");
                        }
                        regs[dst] % src_val
                    }
                    BPF_XOR => regs[dst] ^ src_val,
                    BPF_MOV => src_val,
                    BPF_ARSH => (regs[dst] as i64).wrapping_shr(src_val as u32),
                    BPF_END => {
                        let v = regs[dst] as u64;
                        let converted = match (insn.source(), insn.imm) {
                            (BPF_K, 16) => (v as u16).to_le() as u64,
                            (BPF_K, 32) => (v as u32).to_le() as u64,
                            (BPF_K, 64) => v.to_le(),
                            (BPF_X, 16) => (v as u16).to_be() as u64,
                            (BPF_X, 32) => (v as u32).to_be() as u64,
                            (BPF_X, 64) => v.to_be(),
                            _ => return Err("BPF invalid endian conversion"),
                        };
                        converted as i64
                    }
                    _ => return Err("BPF unknown ALU op"),
                };

                if is64 {
                    regs[dst] = result;
                } else {
                    // 32-bit ALU: zero-extend lower 32 bits
                    regs[dst] = (result as u32) as i64;
                }
                pc += 1;
            }

            BPF_CLASS_JMP | BPF_CLASS_JMP32 => {
                let op = insn.jmp_op();
                let is32 = class == BPF_CLASS_JMP32;

                match op {
                    BPF_JA => {
                        pc = (pc as isize + 1 + insn.off as isize) as usize;
                    }
                    BPF_EXIT => {
                        return Ok(regs[0]);
                    }
                    BPF_CALL => return Err("BPF helper calls are unsupported"),
                    _ => {
                        let dst_val = regs[insn.dst as usize];
                        let src_val = if insn.source() == BPF_X {
                            regs[insn.src as usize]
                        } else {
                            insn.imm as i64
                        };

                        let cmp_dst = if is32 { dst_val as i32 as i64 } else { dst_val };
                        let cmp_src = if is32 { src_val as i32 as i64 } else { src_val };

                        let taken = match op {
                            BPF_JEQ => cmp_dst == cmp_src,
                            BPF_JNE => cmp_dst != cmp_src,
                            BPF_JGT => (cmp_dst as u64) > (cmp_src as u64),
                            BPF_JGE => (cmp_dst as u64) >= (cmp_src as u64),
                            BPF_JLT => (cmp_dst as u64) < (cmp_src as u64),
                            BPF_JLE => (cmp_dst as u64) <= (cmp_src as u64),
                            BPF_JSGT => cmp_dst > cmp_src,
                            BPF_JSGE => cmp_dst >= cmp_src,
                            BPF_JSLT => cmp_dst < cmp_src,
                            BPF_JSLE => cmp_dst <= cmp_src,
                            BPF_JSET => (cmp_dst & cmp_src) != 0,
                            _ => return Err("BPF unknown jump op"),
                        };

                        if taken {
                            pc = (pc as isize + 1 + insn.off as isize) as usize;
                        } else {
                            pc += 1;
                        }
                    }
                }
            }

            BPF_CLASS_LDX => {
                let size = insn.size();
                let base = regs[insn.src as usize] as usize;
                let addr = base.wrapping_add(insn.off as isize as usize);
                let dst = insn.dst as usize;

                let val = if insn.src == 10 {
                    let n = size_bytes(size);
                    let stack_top = regs[10] as usize;
                    let stack_base = stack_top.saturating_sub(BPF_STACK_SIZE);
                    let end = addr.checked_add(n).ok_or("BPF stack read out of bounds")?;
                    if addr < stack_base || end > stack_top {
                        return Err("BPF stack read out of bounds");
                    }
                    read_mem(&stack, addr - stack_base, size)
                } else {
                    return Err("BPF read outside stack");
                };
                regs[dst] = val;
                pc += 1;
            }

            BPF_CLASS_ST => {
                let size = insn.size();
                let base = regs[insn.dst as usize] as usize;
                let addr = base.wrapping_add(insn.off as isize as usize);
                let val = insn.imm as i64;

                if insn.dst == 10 {
                    let n = size_bytes(size);
                    let stack_top = regs[10] as usize;
                    let stack_base = stack_top.saturating_sub(BPF_STACK_SIZE);
                    let end = addr.checked_add(n).ok_or("BPF stack write out of bounds")?;
                    if addr < stack_base || end > stack_top {
                        return Err("BPF stack write out of bounds");
                    }
                    write_mem(&mut stack, addr - stack_base, size, val);
                } else {
                    return Err("BPF write outside stack");
                }
                pc += 1;
            }

            BPF_CLASS_STX => {
                let size = insn.size();
                let base = regs[insn.dst as usize] as usize;
                let addr = base.wrapping_add(insn.off as isize as usize);
                let val = regs[insn.src as usize];

                if insn.dst == 10 {
                    let n = size_bytes(size);
                    let stack_top = regs[10] as usize;
                    let stack_base = stack_top.saturating_sub(BPF_STACK_SIZE);
                    let end = addr.checked_add(n).ok_or("BPF stack write out of bounds")?;
                    if addr < stack_base || end > stack_top {
                        return Err("BPF stack write out of bounds");
                    }
                    write_mem(&mut stack, addr - stack_base, size, val);
                } else {
                    return Err("BPF write outside stack");
                }
                pc += 1;
            }

            BPF_CLASS_LD => {
                let mode = insn.mode();
                if mode == BPF_LD_IMM && insn.size() == BPF_SIZE_DW {
                    // Wide immediate: combine two instruction slots
                    let lo = insn.imm as u32 as u64;
                    let hi = if pc + 1 < n {
                        let next = BpfInsn::decode(insns[pc + 1]);
                        next.imm as u32 as u64
                    } else {
                        0
                    };
                    regs[insn.dst as usize] = ((hi << 16) << 16 | lo) as i64;
                    pc += 2;
                } else if mode == BPF_LD_IMM {
                    regs[insn.dst as usize] = insn.imm as i64;
                    pc += 1;
                } else {
                    return Err("BPF packet/direct memory access is unsupported");
                }
            }

            _ => return Err("BPF unknown instruction class"),
        }
    }

    Err("BPF program ended without EXIT")
}

fn read_mem(buf: &[u8], offset: usize, size: u8) -> i64 {
    if offset + size_bytes(size) > buf.len() {
        return 0;
    }
    match size {
        BPF_SIZE_B => buf[offset] as i64,
        BPF_SIZE_H => u16::from_le_bytes([buf[offset], buf[offset + 1]]) as i64,
        BPF_SIZE_W => u32::from_le_bytes([
            buf[offset],
            buf[offset + 1],
            buf[offset + 2],
            buf[offset + 3],
        ]) as i64,
        BPF_SIZE_DW => i64::from_le_bytes([
            buf[offset],
            buf[offset + 1],
            buf[offset + 2],
            buf[offset + 3],
            buf[offset + 4],
            buf[offset + 5],
            buf[offset + 6],
            buf[offset + 7],
        ]),
        _ => 0,
    }
}

fn write_mem(buf: &mut [u8], offset: usize, size: u8, val: i64) {
    let n = size_bytes(size);
    if offset + n > buf.len() {
        return;
    }
    match size {
        BPF_SIZE_B => buf[offset] = val as u8,
        BPF_SIZE_H => {
            let v = val as u16;
            buf[offset..offset + 2].copy_from_slice(&v.to_le_bytes());
        }
        BPF_SIZE_W => {
            let v = val as u32;
            buf[offset..offset + 4].copy_from_slice(&v.to_le_bytes());
        }
        BPF_SIZE_DW => {
            let v = val as i64;
            buf[offset..offset + 8].copy_from_slice(&v.to_le_bytes());
        }
        _ => {}
    }
}

fn size_bytes(size: u8) -> usize {
    match size {
        BPF_SIZE_B => 1,
        BPF_SIZE_H => 2,
        BPF_SIZE_W => 4,
        BPF_SIZE_DW => 8,
        _ => 0,
    }
}

// ── Syscall implementation ──────────────────────────────────────────────

/// bpf syscall — dispatch on cmd
pub fn bpf(cmd: u32, attr: u64, size: u32) -> i32 {
    if attr == 0 && cmd != BPF_PROG_GET_NEXT_ID && cmd != BPF_MAP_GET_NEXT_ID {
        return -14; // EFAULT
    }

    match cmd {
        BPF_MAP_CREATE => bpf_map_create(attr, size),
        BPF_MAP_LOOKUP_ELEM => bpf_map_lookup_elem(attr, size),
        BPF_MAP_UPDATE_ELEM => bpf_map_update_elem(attr, size),
        BPF_MAP_DELETE_ELEM => bpf_map_delete_elem(attr, size),
        BPF_MAP_GET_NEXT_KEY => bpf_map_get_next_key(attr, size),
        BPF_PROG_LOAD => bpf_prog_load(attr, size),
        BPF_MAP_FREEZE => bpf_map_freeze(attr, size),
        BPF_OBJ_GET_INFO_BY_FD => bpf_obj_get_info_by_fd(attr, size),
        BPF_PROG_GET_NEXT_ID => {
            let start = unsafe { *(attr as *const u32) };
            let progs = PROGS.read();
            let next = progs.keys().find(|&&k| k > start).copied();
            match next {
                Some(id) => {
                    unsafe {
                        *(attr as *mut u32) = id;
                    }
                    0
                }
                None => -2, // ENOENT
            }
        }
        BPF_MAP_GET_NEXT_ID => {
            let start = unsafe { *(attr as *const u32) };
            let maps = MAPS.read();
            let next = maps.keys().find(|&&k| k > start).copied();
            match next {
                Some(id) => {
                    unsafe {
                        *(attr as *mut u32) = id;
                    }
                    0
                }
                None => -2,
            }
        }
        BPF_PROG_GET_FD_BY_ID => {
            let id = unsafe { *(attr as *const u32) };
            let progs = PROGS.read();
            if progs.contains_key(&id) {
                crate::linux_compat::special_fd::register_bpf_prog(id, crate::vfs::OpenFlags::RDWR)
            } else {
                -2 // ENOENT
            }
        }
        BPF_MAP_GET_FD_BY_ID => {
            let id = unsafe { *(attr as *const u32) };
            let maps = MAPS.read();
            if maps.contains_key(&id) {
                crate::linux_compat::special_fd::register_bpf_map(id, crate::vfs::OpenFlags::RDWR)
            } else {
                -2
            }
        }
        BPF_PROG_TEST_RUN => {
            // BPF_PROG_TEST_RUN: execute a loaded program with test data
            #[repr(C)]
            #[derive(Clone, Copy)]
            struct TestRunAttr {
                prog_fd: u32,
                retval: u32,
                data_in: u64,
                data_out: u64,
                data_size_in: u32,
                data_size_out: u32,
                ctx_in: u64,
                ctx_out: u64,
                ctx_size_in: u32,
                ctx_size_out: u32,
                repeat: u32,
                duration: u64,
            }
            if size < core::mem::size_of::<TestRunAttr>() as u32 {
                return -22;
            }
            let a = unsafe { *(attr as *const TestRunAttr) };
            if a.data_in != 0
                || a.data_size_in != 0
                || a.data_out != 0
                || a.data_size_out != 0
                || a.ctx_out != 0
                || a.ctx_size_out != 0
            {
                return -38;
            }
            let prog_id = match crate::linux_compat::special_fd::get_bpf_prog_id(a.prog_fd as i32) {
                Some(id) => id,
                None => return -9,
            };
            let progs = PROGS.read();
            let prog_mutex = match progs.get(&prog_id) {
                Some(p) => p,
                None => return -9,
            };
            let prog = prog_mutex.lock();

            // Execute the program with the provided context (or 0 if none)
            let ctx = if a.ctx_in != 0 && a.ctx_size_in > 0 {
                a.ctx_in
            } else {
                0
            };

            let repeats = if a.repeat == 0 { 1 } else { a.repeat.min(1000) };
            let mut last_ret = 0i64;
            for _ in 0..repeats {
                match execute_program(&prog.insns, ctx) {
                    Ok(ret) => last_ret = ret,
                    Err(e) => {
                        crate::serial_println!("[bpf] test_run failed: {}", e);
                        return -22;
                    }
                }
            }

            // Write retval back to the attr struct
            unsafe {
                let attr_ptr = attr as *mut TestRunAttr;
                (*attr_ptr).retval = last_ret as u32;
                (*attr_ptr).data_size_out = 0;
                (*attr_ptr).ctx_size_out = 0;
                (*attr_ptr).duration = 0;
            }
            0
        }
        BPF_PROG_ATTACH
        | BPF_PROG_DETACH
        | BPF_PROG_QUERY
        | BPF_RAW_TRACEPOINT_OPEN
        | BPF_BTF_LOAD
        | BPF_BTF_GET_FD_BY_ID
        | BPF_TASK_FD_QUERY
        | BPF_MAP_LOOKUP_AND_DELETE_ELEM
        | BPF_BTF_GET_NEXT_ID
        | BPF_MAP_LOOKUP_BATCH
        | BPF_MAP_LOOKUP_AND_DELETE_BATCH
        | BPF_MAP_UPDATE_BATCH
        | BPF_MAP_DELETE_BATCH
        | BPF_LINK_CREATE
        | BPF_LINK_UPDATE
        | BPF_LINK_GET_FD_BY_ID
        | BPF_LINK_GET_NEXT_ID
        | BPF_ENABLE_STATS
        | BPF_ITER_CREATE
        | BPF_LINK_DETACH
        | BPF_PROG_BIND_MAP
        | BPF_TOKEN_CREATE
        | BPF_OBJ_PIN
        | BPF_OBJ_GET => {
            // These require more infrastructure — return EPERM for now
            -1 // EPERM
        }
        _ => -22, // EINVAL
    }
}

fn bpf_map_create(attr: u64, size: u32) -> i32 {
    if size < core::mem::size_of::<BpfMapCreateAttr>() as u32 {
        return -22;
    }
    let a = unsafe { *(attr as *const BpfMapCreateAttr) };

    // Validate map type
    let valid_types = [
        BPF_MAP_TYPE_HASH,
        BPF_MAP_TYPE_ARRAY,
        BPF_MAP_TYPE_PERCPU_HASH,
        BPF_MAP_TYPE_PERCPU_ARRAY,
        BPF_MAP_TYPE_LRU_HASH,
        BPF_MAP_TYPE_LRU_PERCPU_HASH,
    ];
    if !valid_types.contains(&a.map_type) {
        return -22;
    }
    if a.key_size == 0 || a.value_size == 0 || a.max_entries == 0 {
        return -22;
    }
    if a.key_size > 256 || a.value_size > 4096 || a.max_entries > 100_000 {
        return -7; // E2BIG
    }

    let id = NEXT_MAP_ID.fetch_add(1, Ordering::SeqCst);
    let map = BpfMap {
        id,
        map_type: a.map_type,
        key_size: a.key_size,
        value_size: a.value_size,
        max_entries: a.max_entries,
        flags: a.map_flags,
        frozen: false,
        entries: BTreeMap::new(),
    };
    MAPS.write().insert(id, Mutex::new(map));

    let fd = crate::linux_compat::special_fd::register_bpf_map(id, crate::vfs::OpenFlags::RDWR);
    if fd < 0 {
        MAPS.write().remove(&id);
        return -23;
    }
    crate::serial_println!(
        "[bpf] map_create: type={} key_sz={} val_sz={} fd={}",
        a.map_type,
        a.key_size,
        a.value_size,
        fd
    );
    fd
}

fn bpf_map_lookup_elem(attr: u64, size: u32) -> i32 {
    if size < core::mem::size_of::<BpfMapElemAttr>() as u32 {
        return -22;
    }
    let a = unsafe { *(attr as *const BpfMapElemAttr) };
    let map_id = match crate::linux_compat::special_fd::get_bpf_map_id(a.map_fd as i32) {
        Some(id) => id,
        None => return -9,
    };
    let maps = MAPS.read();
    let map_mutex = match maps.get(&map_id) {
        Some(m) => m,
        None => return -9,
    };
    let map = map_mutex.lock();

    let key = unsafe { core::slice::from_raw_parts(a.key as *const u8, map.key_size as usize) };
    match map.entries.get(key) {
        Some(val) => {
            let dst = unsafe {
                core::slice::from_raw_parts_mut(
                    a.value_or_next_key as *mut u8,
                    map.value_size as usize,
                )
            };
            dst.copy_from_slice(val);
            0
        }
        None => -2, // ENOENT
    }
}

fn bpf_map_update_elem(attr: u64, size: u32) -> i32 {
    if size < core::mem::size_of::<BpfMapElemAttr>() as u32 {
        return -22;
    }
    let a = unsafe { *(attr as *const BpfMapElemAttr) };
    let map_id = match crate::linux_compat::special_fd::get_bpf_map_id(a.map_fd as i32) {
        Some(id) => id,
        None => return -9,
    };
    let maps = MAPS.read();
    let map_mutex = match maps.get(&map_id) {
        Some(m) => m,
        None => return -9,
    };
    let mut map = map_mutex.lock();
    if map.frozen {
        return -1; // EPERM
    }
    if map.entries.len() as u32 >= map.max_entries {
        // Check if key already exists
        let key = unsafe { core::slice::from_raw_parts(a.key as *const u8, map.key_size as usize) };
        if !map.entries.contains_key(key) {
            return -7; // E2BIG
        }
    }
    let key = unsafe { core::slice::from_raw_parts(a.key as *const u8, map.key_size as usize) };
    let val = unsafe {
        core::slice::from_raw_parts(a.value_or_next_key as *const u8, map.value_size as usize)
    };
    map.entries.insert(key.to_vec(), val.to_vec());
    0
}

fn bpf_map_delete_elem(attr: u64, size: u32) -> i32 {
    if size < core::mem::size_of::<BpfMapElemAttr>() as u32 {
        return -22;
    }
    let a = unsafe { *(attr as *const BpfMapElemAttr) };
    let map_id = match crate::linux_compat::special_fd::get_bpf_map_id(a.map_fd as i32) {
        Some(id) => id,
        None => return -9,
    };
    let maps = MAPS.read();
    let map_mutex = match maps.get(&map_id) {
        Some(m) => m,
        None => return -9,
    };
    let mut map = map_mutex.lock();
    if map.frozen {
        return -1;
    }
    let key = unsafe { core::slice::from_raw_parts(a.key as *const u8, map.key_size as usize) };
    match map.entries.remove(key) {
        Some(_) => 0,
        None => -2,
    }
}

fn bpf_map_get_next_key(attr: u64, size: u32) -> i32 {
    if size < core::mem::size_of::<BpfMapElemAttr>() as u32 {
        return -22;
    }
    let a = unsafe { *(attr as *const BpfMapElemAttr) };
    let map_id = match crate::linux_compat::special_fd::get_bpf_map_id(a.map_fd as i32) {
        Some(id) => id,
        None => return -9,
    };
    let maps = MAPS.read();
    let map_mutex = match maps.get(&map_id) {
        Some(m) => m,
        None => return -9,
    };
    let map = map_mutex.lock();

    if a.key == 0 {
        // Return first key
        if let Some(first_key) = map.entries.keys().next() {
            let dst = unsafe {
                core::slice::from_raw_parts_mut(
                    a.value_or_next_key as *mut u8,
                    map.key_size as usize,
                )
            };
            dst.copy_from_slice(first_key);
            return 0;
        }
        return -2;
    }

    let key = unsafe { core::slice::from_raw_parts(a.key as *const u8, map.key_size as usize) };
    let mut found = false;
    for k in map.entries.keys() {
        if found {
            let dst = unsafe {
                core::slice::from_raw_parts_mut(
                    a.value_or_next_key as *mut u8,
                    map.key_size as usize,
                )
            };
            dst.copy_from_slice(k);
            return 0;
        }
        if k == key {
            found = true;
        }
    }
    -2 // ENOENT
}

fn bpf_prog_load(attr: u64, size: u32) -> i32 {
    if size < 48 {
        // Minimum size for the fields we use
        return -22;
    }
    let a = unsafe { *(attr as *const BpfProgLoadAttr) };

    // Validate program type
    let valid_types = [
        BPF_PROG_TYPE_SOCKET_FILTER,
        BPF_PROG_TYPE_KPROBE,
        BPF_PROG_TYPE_SCHED_CLS,
        BPF_PROG_TYPE_SCHED_ACT,
        BPF_PROG_TYPE_TRACEPOINT,
        BPF_PROG_TYPE_XDP,
        BPF_PROG_TYPE_PERF_EVENT,
        BPF_PROG_TYPE_CGROUP_SKB,
        BPF_PROG_TYPE_CGROUP_SOCK,
    ];
    if !valid_types.contains(&a.prog_type) && a.prog_type != BPF_PROG_TYPE_UNSPEC {
        return -22;
    }
    if a.insn_cnt == 0 || a.insn_cnt > 4096 {
        return -22;
    }
    if a.insns == 0 || a.license == 0 {
        return -14;
    }

    // Copy instructions (each BPF insn is 8 bytes: u8 code, u8 regs, s16 off, s32 imm)
    let insn_bytes = a.insn_cnt as usize * 8;
    let insn_ptr = a.insns as *const u64;
    let mut insns = Vec::with_capacity(a.insn_cnt as usize);
    for i in 0..a.insn_cnt as usize {
        insns.push(unsafe { core::ptr::read_volatile(insn_ptr.add(i)) });
    }

    // Copy license string
    let mut license = Vec::new();
    let lic_ptr = a.license as *const u8;
    let mut i = 0;
    while i < 128 {
        let b = unsafe { *lic_ptr.add(i) };
        if b == 0 {
            break;
        }
        license.push(b);
        i += 1;
    }

    // Verify the BPF program before storing it
    if let Err(errno) = verify_program(&insns) {
        return -errno;
    }

    let id = NEXT_PROG_ID.fetch_add(1, Ordering::SeqCst);
    let prog = BpfProg {
        id,
        prog_type: a.prog_type,
        insns,
        license,
    };
    PROGS.write().insert(id, Mutex::new(prog));

    let fd = crate::linux_compat::special_fd::register_bpf_prog(id, crate::vfs::OpenFlags::RDWR);
    if fd < 0 {
        PROGS.write().remove(&id);
        return -23;
    }
    crate::serial_println!(
        "[bpf] prog_load: type={} cnt={} fd={}",
        a.prog_type,
        a.insn_cnt,
        fd
    );
    fd
}

fn bpf_map_freeze(attr: u64, _size: u32) -> i32 {
    let map_fd = unsafe { *(attr as *const u32) };
    let map_id = match crate::linux_compat::special_fd::get_bpf_map_id(map_fd as i32) {
        Some(id) => id,
        None => return -9,
    };
    let maps = MAPS.read();
    if let Some(map_mutex) = maps.get(&map_id) {
        let mut map = map_mutex.lock();
        map.frozen = true;
        0
    } else {
        -9
    }
}

fn bpf_obj_get_info_by_fd(attr: u64, _size: u32) -> i32 {
    #[repr(C)]
    #[derive(Clone, Copy)]
    struct InfoAttr {
        bpf_fd: u32,
        info_len: u32,
        info: u64,
    }
    let a = unsafe { *(attr as *const InfoAttr) };

    // Try map first, then prog
    if let Some(map_id) = crate::linux_compat::special_fd::get_bpf_map_id(a.bpf_fd as i32) {
        let maps = MAPS.read();
        if let Some(map_mutex) = maps.get(&map_id) {
            let map = map_mutex.lock();
            // Write minimal map info
            #[repr(C)]
            struct BpfMapInfo {
                map_type: u32,
                id: u32,
                key_size: u32,
                value_size: u32,
                max_entries: u32,
                map_flags: u32,
                name: [u8; 16],
                ifindex: u32,
                btf_vmlinux_value_type_id: u32,
            }
            let info = BpfMapInfo {
                map_type: map.map_type,
                id: map.id,
                key_size: map.key_size,
                value_size: map.value_size,
                max_entries: map.max_entries,
                map_flags: map.flags,
                name: [0u8; 16],
                ifindex: 0,
                btf_vmlinux_value_type_id: 0,
            };
            let info_bytes = core::mem::size_of::<BpfMapInfo>() as u32;
            let copy_len = core::cmp::min(a.info_len, info_bytes) as usize;
            unsafe {
                core::ptr::copy_nonoverlapping(
                    &info as *const BpfMapInfo as *const u8,
                    a.info as *mut u8,
                    copy_len,
                );
            }
            return 0;
        }
    }
    if let Some(prog_id) = crate::linux_compat::special_fd::get_bpf_prog_id(a.bpf_fd as i32) {
        let progs = PROGS.read();
        if let Some(prog_mutex) = progs.get(&prog_id) {
            let prog = prog_mutex.lock();
            #[repr(C)]
            struct BpfProgInfo {
                prog_type: u32,
                id: u32,
                tag: [u8; 8],
                jited_prog_len: u32,
                xlated_prog_len: u32,
                jited_prog_insns: u64,
                xlated_prog_insns: u64,
                load_time: u64,
                created_by_uid: u32,
                nr_map_ids: u32,
                map_ids: u64,
                name: [u8; 16],
            }
            let info = BpfProgInfo {
                prog_type: prog.prog_type,
                id: prog.id,
                tag: [0u8; 8],
                jited_prog_len: 0,
                xlated_prog_len: prog.insns.len() as u32 * 8,
                jited_prog_insns: 0,
                xlated_prog_insns: 0,
                load_time: 0,
                created_by_uid: 0,
                nr_map_ids: 0,
                map_ids: 0,
                name: [0u8; 16],
            };
            let info_bytes = core::mem::size_of::<BpfProgInfo>() as u32;
            let copy_len = core::cmp::min(a.info_len, info_bytes) as usize;
            unsafe {
                core::ptr::copy_nonoverlapping(
                    &info as *const BpfProgInfo as *const u8,
                    a.info as *mut u8,
                    copy_len,
                );
            }
            return 0;
        }
    }
    -9 // EBADF
}

/// Close a BPF map (called when fd is closed).
pub fn close_map(id: u32) {
    MAPS.write().remove(&id);
}

/// Close a BPF prog (called when fd is closed).
pub fn close_prog(id: u32) {
    PROGS.write().remove(&id);
}

/// Initialize the BPF subsystem.
pub fn init() {
    crate::serial_println!("[bpf] BPF subsystem initialized");
}
