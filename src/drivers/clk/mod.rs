//! Clock framework (clk subsystem)
//!
//! Provides clock tree management with parent/child relationships, rate
//! calculation, enable/disable gating, and provider registration. Mirrors
//! Linux's `drivers/clk/clk.c` with the common clock framework (CCF).

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, Ordering};
use spin::RwLock;

// ── Types ───────────────────────────────────────────────────────────────

/// Clock operations (Linux `struct clk_ops`).
pub struct ClkOps {
    pub enable: Option<fn() -> Result<(), &'static str>>,
    pub disable: Option<fn() -> Result<(), &'static str>>,
    pub is_enabled: Option<fn() -> bool>,
    pub recalc_rate: Option<fn(parent_rate: u64) -> u64>,
    pub round_rate: Option<fn(rate: u64, parent_rate: u64) -> u64>,
    pub set_rate: Option<fn(rate: u64, parent_rate: u64) -> Result<u64, &'static str>>,
    pub set_parent: Option<fn(parent_id: u32) -> Result<(), &'static str>>,
    pub get_parent: Option<fn() -> u32>,
    pub get_name: fn() -> &'static str,
}

struct ClkNode {
    id: u32,
    name: String,
    parent_id: Option<u32>,
    children: Vec<u32>,
    ops: &'static ClkOps,
    rate: u64,
    enabled: bool,
    enable_count: u32,
    prepare_count: u32,
}

// ── Fixed clock (always-on, fixed rate) ─────────────────────────────────

fn fixed_noop() -> Result<(), &'static str> {
    Ok(())
}
fn fixed_enabled() -> bool {
    true
}

fn fixed_rate_fn() -> u64 {
    unsafe { FIXED_CLOCK_RATE }
}

static mut FIXED_CLOCK_RATE: u64 = 25_000_000; // 25 MHz default

fn fixed_recalc(parent_rate: u64) -> u64 {
    let _ = parent_rate;
    unsafe { FIXED_CLOCK_RATE }
}

fn fixed_name() -> &'static str {
    "fixed-clk"
}

pub static FIXED_CLK_OPS: ClkOps = ClkOps {
    enable: Some(fixed_noop),
    disable: Some(fixed_noop),
    is_enabled: Some(fixed_enabled),
    recalc_rate: Some(fixed_recalc),
    round_rate: None,
    set_rate: None,
    set_parent: None,
    get_parent: None,
    get_name: fixed_name,
};

// ── CPU PLL clock (derived from fixed clock) ────────────────────────────

static mut CPU_PLL_RATE: u64 = 3_200_000_000; // 3.2 GHz

fn cpu_pll_recalc(parent_rate: u64) -> u64 {
    // PLL multiplies parent by a fixed factor
    let _ = parent_rate;
    unsafe { CPU_PLL_RATE }
}

fn cpu_pll_round(rate: u64, _parent_rate: u64) -> u64 {
    // Round to nearest 100 MHz step
    ((rate + 50_000_000) / 100_000_000) * 100_000_000
}

fn cpu_pll_set_rate(rate: u64, _parent_rate: u64) -> Result<u64, &'static str> {
    let rounded = cpu_pll_round(rate, 0);
    unsafe {
        CPU_PLL_RATE = rounded;
    }
    Ok(rounded)
}

fn cpu_pll_name() -> &'static str {
    "cpu-pll"
}

pub static CPU_PLL_OPS: ClkOps = ClkOps {
    enable: Some(fixed_noop),
    disable: Some(fixed_noop),
    is_enabled: Some(fixed_enabled),
    recalc_rate: Some(cpu_pll_recalc),
    round_rate: Some(cpu_pll_round),
    set_rate: Some(cpu_pll_set_rate),
    set_parent: None,
    get_parent: None,
    get_name: cpu_pll_name,
};

// ── Peripheral clock (gatable divider from CPU PLL) ─────────────────────

static mut PERIPH_DIVIDER: u32 = 8;

fn periph_recalc(parent_rate: u64) -> u64 {
    let div = unsafe { PERIPH_DIVIDER } as u64;
    if div == 0 {
        parent_rate
    } else {
        parent_rate / div
    }
}

fn periph_round(rate: u64, parent_rate: u64) -> u64 {
    if rate >= parent_rate {
        return parent_rate;
    }
    let div = (parent_rate + rate / 2) / rate;
    let div = div.clamp(1, 256);
    parent_rate / div
}

fn periph_set_rate(rate: u64, parent_rate: u64) -> Result<u64, &'static str> {
    if rate > parent_rate {
        return Err("Peripheral clock rate cannot exceed parent");
    }
    let div = (parent_rate + rate / 2) / rate;
    let div = div.clamp(1, 256) as u32;
    unsafe {
        PERIPH_DIVIDER = div;
    }
    Ok(parent_rate / div as u64)
}

fn periph_enable() -> Result<(), &'static str> {
    Ok(())
}
fn periph_disable() -> Result<(), &'static str> {
    Ok(())
}

static mut PERIPH_ENABLED: bool = false;
fn periph_is_enabled() -> bool {
    unsafe { PERIPH_ENABLED }
}

fn periph_name() -> &'static str {
    "peripheral-clk"
}

pub static PERIPH_CLK_OPS: ClkOps = ClkOps {
    enable: Some(periph_enable),
    disable: Some(periph_disable),
    is_enabled: Some(periph_is_enabled),
    recalc_rate: Some(periph_recalc),
    round_rate: Some(periph_round),
    set_rate: Some(periph_set_rate),
    set_parent: None,
    get_parent: None,
    get_name: periph_name,
};

// ── Registry ────────────────────────────────────────────────────────────

static CLK_REGISTRY: RwLock<BTreeMap<u32, ClkNode>> = RwLock::new(BTreeMap::new());
static NEXT_CLK_ID: AtomicU32 = AtomicU32::new(0);

// ── Public API ──────────────────────────────────────────────────────────

/// Register a clock (Linux `clk_register`).
pub fn register_clock(
    name: &str,
    parent_id: Option<u32>,
    ops: &'static ClkOps,
) -> Result<u32, &'static str> {
    let id = NEXT_CLK_ID.fetch_add(1, Ordering::SeqCst);

    // Calculate initial rate.
    let parent_rate = parent_id
        .and_then(|pid| CLK_REGISTRY.read().get(&pid).map(|p| p.rate))
        .unwrap_or(0);

    let rate = if let Some(recalc) = ops.recalc_rate {
        recalc(parent_rate)
    } else {
        parent_rate
    };

    let enabled = ops.is_enabled.map_or(false, |f| f());

    CLK_REGISTRY.write().insert(
        id,
        ClkNode {
            id,
            name: String::from(name),
            parent_id,
            children: Vec::new(),
            ops,
            rate,
            enabled,
            enable_count: 0,
            prepare_count: 0,
        },
    );

    // Add to parent's children list.
    if let Some(pid) = parent_id {
        if let Some(parent) = CLK_REGISTRY.write().get_mut(&pid) {
            parent.children.push(id);
        }
    }

    Ok(id)
}

/// Enable a clock (Linux `clk_prepare_enable`).
pub fn enable(clk_id: u32) -> Result<(), &'static str> {
    let (ops, parent_id) = {
        let reg = CLK_REGISTRY.read();
        let clk = reg.get(&clk_id).ok_or("Clock not found")?;
        (clk.ops, clk.parent_id)
    };

    // Enable parent first (propagation).
    if let Some(pid) = parent_id {
        enable(pid)?;
    }

    let mut reg = CLK_REGISTRY.write();
    let clk = reg.get_mut(&clk_id).ok_or("Clock vanished")?;
    clk.enable_count += 1;
    if clk.enable_count == 1 && !clk.enabled {
        if let Some(enable_fn) = ops.enable {
            enable_fn()?;
        }
        clk.enabled = true;
    }
    Ok(())
}

/// Disable a clock (Linux `clk_disable_unprepare`).
pub fn disable(clk_id: u32) -> Result<(), &'static str> {
    let (ops, parent_id) = {
        let reg = CLK_REGISTRY.read();
        let clk = reg.get(&clk_id).ok_or("Clock not found")?;
        if clk.enable_count == 0 {
            return Err("Clock already disabled");
        }
        (clk.ops, clk.parent_id)
    };

    let mut reg = CLK_REGISTRY.write();
    let clk = reg.get_mut(&clk_id).ok_or("Clock vanished")?;
    clk.enable_count -= 1;
    if clk.enable_count == 0 && clk.enabled {
        if let Some(disable_fn) = ops.disable {
            disable_fn()?;
        }
        clk.enabled = false;
    }
    drop(reg);

    // Disable parent if its enable count reaches zero.
    if let Some(pid) = parent_id {
        let should_disable = {
            let reg = CLK_REGISTRY.read();
            reg.get(&pid)
                .map_or(false, |p| p.enable_count == 0 && p.enabled)
        };
        if should_disable {
            let _ = disable(pid);
        }
    }
    Ok(())
}

/// Get clock rate (Linux `clk_get_rate`).
pub fn get_rate(clk_id: u32) -> Result<u64, &'static str> {
    let reg = CLK_REGISTRY.read();
    let clk = reg.get(&clk_id).ok_or("Clock not found")?;
    Ok(clk.rate)
}

/// Set clock rate (Linux `clk_set_rate`).
pub fn set_rate(clk_id: u32, rate: u64) -> Result<u64, &'static str> {
    let (ops, parent_id, parent_rate) = {
        let reg = CLK_REGISTRY.read();
        let clk = reg.get(&clk_id).ok_or("Clock not found")?;
        let pr = clk
            .parent_id
            .and_then(|pid| reg.get(&pid).map(|p| p.rate))
            .unwrap_or(0);
        (clk.ops, clk.parent_id, pr)
    };

    let new_rate = if let Some(set_rate_fn) = ops.set_rate {
        set_rate_fn(rate, parent_rate)?
    } else if let Some(round_fn) = ops.round_rate {
        round_fn(rate, parent_rate)
    } else {
        return Err("Clock rate is not adjustable");
    };

    // Update this clock's rate and propagate to children.
    let mut reg = CLK_REGISTRY.write();
    if let Some(clk) = reg.get_mut(&clk_id) {
        clk.rate = new_rate;
    }

    // Recalculate children rates.
    let children: Vec<u32> = reg.get(&clk_id).map_or(Vec::new(), |c| c.children.clone());
    drop(reg);

    for child_id in children {
        let _ = recalc_rate(child_id);
    }

    let _ = parent_id;
    Ok(new_rate)
}

/// Recalculate rate from parent (Linux `clk_recalc_rate`).
fn recalc_rate(clk_id: u32) -> Result<u64, &'static str> {
    let (ops, parent_rate, children) = {
        let reg = CLK_REGISTRY.read();
        let clk = reg.get(&clk_id).ok_or("Clock not found")?;
        let pr = clk
            .parent_id
            .and_then(|pid| reg.get(&pid).map(|p| p.rate))
            .unwrap_or(0);
        (clk.ops, pr, clk.children.clone())
    };

    let new_rate = if let Some(recalc) = ops.recalc_rate {
        recalc(parent_rate)
    } else {
        parent_rate
    };

    let mut reg = CLK_REGISTRY.write();
    if let Some(clk) = reg.get_mut(&clk_id) {
        clk.rate = new_rate;
    }
    drop(reg);

    // Propagate to children.
    for child_id in children {
        let _ = recalc_rate(child_id);
    }
    Ok(new_rate)
}

/// Get clock parent (Linux `clk_get_parent`).
pub fn get_parent(clk_id: u32) -> Result<Option<u32>, &'static str> {
    let reg = CLK_REGISTRY.read();
    let clk = reg.get(&clk_id).ok_or("Clock not found")?;
    Ok(clk.parent_id)
}

/// Set clock parent (Linux `clk_set_parent`).
pub fn set_parent(clk_id: u32, new_parent_id: u32) -> Result<(), &'static str> {
    let (ops, old_parent_id) = {
        let mut reg = CLK_REGISTRY.write();
        let clk = reg.get_mut(&clk_id).ok_or("Clock not found")?;
        let old_pid = clk.parent_id;
        clk.parent_id = Some(new_parent_id);
        (clk.ops, old_pid)
    };

    // Remove from old parent's children list.
    if let Some(old_pid) = old_parent_id {
        let mut reg = CLK_REGISTRY.write();
        if let Some(old_parent) = reg.get_mut(&old_pid) {
            old_parent.children.retain(|&c| c != clk_id);
        }
    }

    // Call set_parent op if available.
    if let Some(set_parent_fn) = ops.set_parent {
        set_parent_fn(new_parent_id)?;
    }

    // Add to new parent's children.
    {
        let mut reg = CLK_REGISTRY.write();
        if let Some(parent) = reg.get_mut(&new_parent_id) {
            parent.children.push(clk_id);
        }
    }

    // Recalculate rate with new parent.
    let _ = recalc_rate(clk_id);
    Ok(())
}

/// Get clock name.
pub fn get_name(clk_id: u32) -> Result<String, &'static str> {
    let reg = CLK_REGISTRY.read();
    let clk = reg.get(&clk_id).ok_or("Clock not found")?;
    Ok(clk.name.clone())
}

/// Find a clock by name (Linux `clk_get`).
pub fn find_by_name(name: &str) -> Option<u32> {
    CLK_REGISTRY
        .read()
        .iter()
        .find(|(_, clk)| clk.name == name)
        .map(|(id, _)| *id)
}

/// Number of registered clocks.
pub fn clock_count() -> usize {
    CLK_REGISTRY.read().len()
}

/// Get all clock IDs.
pub fn get_all_clocks() -> Vec<u32> {
    CLK_REGISTRY.read().keys().copied().collect()
}

/// Initialize clock subsystem with a default clock tree.
pub fn init() -> Result<(), &'static str> {
    if !CLK_REGISTRY.read().is_empty() {
        return Ok(());
    }

    // Register fixed root clock (25 MHz crystal oscillator).
    let fixed_id = register_clock("osc-25mhz", None, &FIXED_CLK_OPS)?;

    // Register CPU PLL (multiplies oscillator to CPU core frequency).
    let pll_id = register_clock("cpu-pll", Some(fixed_id), &CPU_PLL_OPS)?;

    // Register peripheral clock (divides PLL for peripheral bus).
    let _periph_id = register_clock("peripheral-clk", Some(pll_id), &PERIPH_CLK_OPS)?;

    crate::serial_println!("clk: {} clock(s) registered", clock_count());
    Ok(())
}
