//! Kernel probes — software kprobes with pre/post handlers at symbol addresses.
//!
//! Probes are registered by symbol name or address. Handlers run when
//! [`run_probes_at`] is invoked at instrumented call sites (syscall dispatch,
//! file I/O, etc.). Hits are recorded in the trace ring buffer and can stop
//! ptrace tracees configured for single-step debugging.

extern crate alloc;

use alloc::collections::BTreeMap;
use alloc::format;
use alloc::string::{String, ToString};
use core::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use spin::RwLock;

pub type KprobePreHandler = fn(&mut KprobeContext) -> KprobeAction;
pub type KprobePostHandler = fn(&mut KprobeContext);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KprobeAction {
    Continue,
    SkipPost,
}

#[derive(Debug, Clone)]
pub struct KprobeContext {
    pub probe_id: u32,
    pub symbol: String,
    pub address: u64,
    pub pid: u32,
    pub uid: u32,
    pub data: u64,
    pub pre_result: i64,
}

#[derive(Debug, Clone)]
struct Kprobe {
    id: u32,
    symbol: String,
    address: u64,
    enabled: bool,
    pre_handler: Option<KprobePreHandler>,
    post_handler: Option<KprobePostHandler>,
    hit_count: u64,
}

static NEXT_PROBE_ID: AtomicU32 = AtomicU32::new(1);
static PROBE_HITS: AtomicU64 = AtomicU64::new(0);

static SYMBOLS: RwLock<BTreeMap<String, u64>> = RwLock::new(BTreeMap::new());
static PROBES: RwLock<BTreeMap<u32, Kprobe>> = RwLock::new(BTreeMap::new());
static ADDR_TO_PROBE: RwLock<BTreeMap<u64, u32>> = RwLock::new(BTreeMap::new());

/// Register a kernel symbol name → address mapping for kprobe lookup.
pub fn register_symbol(name: &str, address: u64) {
    SYMBOLS.write().insert(name.to_string(), address);
}

pub fn lookup_symbol(name: &str) -> Option<u64> {
    SYMBOLS.read().get(name).copied()
}

/// Register a kprobe at a symbol name or explicit address.
pub fn register_kprobe(
    symbol: &str,
    address: Option<u64>,
    pre: Option<KprobePreHandler>,
    post: Option<KprobePostHandler>,
) -> Result<u32, i32> {
    if pre.is_none() && post.is_none() {
        return Err(-22); // EINVAL
    }

    let addr = match address {
        Some(a) => a,
        None => lookup_symbol(symbol).ok_or(-2)?, // ENOENT
    };

    if PROBES
        .read()
        .values()
        .any(|p| p.address == addr && p.enabled)
    {
        return Err(-17); // EEXIST
    }

    let id = NEXT_PROBE_ID.fetch_add(1, Ordering::SeqCst);
    let probe = Kprobe {
        id,
        symbol: symbol.to_string(),
        address: addr,
        enabled: true,
        pre_handler: pre,
        post_handler: post,
        hit_count: 0,
    };

    PROBES.write().insert(id, probe);
    ADDR_TO_PROBE.write().insert(addr, id);
    Ok(id)
}

pub fn unregister_kprobe(id: u32) -> bool {
    let mut probes = PROBES.write();
    if let Some(probe) = probes.remove(&id) {
        ADDR_TO_PROBE.write().remove(&probe.address);
        true
    } else {
        false
    }
}

pub fn enable_kprobe(id: u32) -> bool {
    if let Some(probe) = PROBES.write().get_mut(&id) {
        probe.enabled = true;
        true
    } else {
        false
    }
}

pub fn disable_kprobe(id: u32) -> bool {
    if let Some(probe) = PROBES.write().get_mut(&id) {
        probe.enabled = false;
        true
    } else {
        false
    }
}

pub fn probe_count() -> usize {
    PROBES.read().len()
}

pub fn hit_count() -> u64 {
    PROBE_HITS.load(Ordering::Relaxed)
}

/// Run pre/post handlers for any probe registered at `address`.
pub fn run_probes_at(address: u64, data: u64) {
    let probe_id = {
        let map = ADDR_TO_PROBE.read();
        match map.get(&address) {
            Some(id) => *id,
            None => return,
        }
    };

    let mut probes = PROBES.write();
    let probe = match probes.get_mut(&probe_id) {
        Some(p) if p.enabled => p,
        _ => return,
    };

    probe.hit_count += 1;
    PROBE_HITS.fetch_add(1, Ordering::Relaxed);

    let (pid, uid) = current_creds();
    let symbol = probe.symbol.clone();
    let pre = probe.pre_handler;
    let post = probe.post_handler;

    let mut ctx = KprobeContext {
        probe_id,
        symbol: symbol.clone(),
        address,
        pid,
        uid,
        data,
        pre_result: 0,
    };

    let skip_post = if let Some(pre_fn) = pre {
        matches!(pre_fn(&mut ctx), KprobeAction::SkipPost)
    } else {
        false
    };

    crate::trace::tracepoint_emit("kprobes:probe_hit", address);
    crate::trace::record_function_trace(&format!("kprobe:{}", symbol), address, data);

    notify_ptrace_on_hit(pid, probe_id, address);

    if !skip_post {
        if let Some(post_fn) = post {
            post_fn(&mut ctx);
        }
    }
}

fn current_creds() -> (u32, u32) {
    let pid = crate::process::current_pid();
    if pid == 0 {
        return (0, 0);
    }
    let pm = crate::process::get_process_manager();
    if let Some(pcb) = pm.get_process(pid) {
        (pid, pcb.euid)
    } else {
        (pid, 0)
    }
}

fn notify_ptrace_on_hit(pid: u32, probe_id: u32, address: u64) {
    if !crate::ptrace::is_traced(pid) {
        return;
    }
    crate::ptrace::kprobe_event(pid, probe_id, address);
}

fn pre_log_syscall(ctx: &mut KprobeContext) -> KprobeAction {
    ctx.pre_result = ctx.data as i64;
    KprobeAction::Continue
}

fn post_log_syscall(ctx: &mut KprobeContext) {
    if crate::audit::is_enabled() {
        crate::serial_println!(
            "[kprobe] {} pid={} addr={:#x}",
            ctx.symbol,
            ctx.pid,
            ctx.address
        );
    }
}

fn pre_log_file(_ctx: &mut KprobeContext) -> KprobeAction {
    KprobeAction::Continue
}

fn post_log_file(ctx: &mut KprobeContext) {
    crate::trace::record_function_trace(&ctx.symbol, ctx.address, ctx.data);
}

fn register_builtin_symbols() {
    register_symbol(
        "dispatch_syscall",
        crate::syscall_handler::dispatch_syscall as *const () as u64,
    );
    register_symbol(
        "file_ops_open",
        crate::linux_compat::file_ops::open as *const () as u64,
    );
    register_symbol(
        "file_ops_read",
        crate::linux_compat::file_ops::read as *const () as u64,
    );
    register_symbol(
        "file_ops_write",
        crate::linux_compat::file_ops::write as *const () as u64,
    );
}

fn register_default_probes() {
    let _ = register_kprobe(
        "dispatch_syscall",
        None,
        Some(pre_log_syscall),
        Some(post_log_syscall),
    );
    let _ = register_kprobe(
        "file_ops_open",
        None,
        Some(pre_log_file),
        Some(post_log_file),
    );
}

pub fn init() {
    SYMBOLS.write().clear();
    PROBES.write().clear();
    ADDR_TO_PROBE.write().clear();
    NEXT_PROBE_ID.store(1, Ordering::SeqCst);
    PROBE_HITS.store(0, Ordering::SeqCst);

    register_builtin_symbols();
    register_default_probes();

    crate::serial_println!(
        "[kprobes] initialized ({} probes, {} symbols)",
        probe_count(),
        SYMBOLS.read().len()
    );
}

pub fn list_probes_text() -> String {
    let probes = PROBES.read();
    let mut out = String::new();
    for p in probes.values() {
        out.push_str(&format!(
            "id={} symbol={} addr={:#x} enabled={} hits={}\n",
            p.id,
            p.symbol,
            p.address,
            if p.enabled { 1 } else { 0 },
            p.hit_count
        ));
    }
    out
}
