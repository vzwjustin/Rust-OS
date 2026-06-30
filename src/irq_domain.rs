//! IRQ domain framework
//!
//! Provides hierarchical interrupt controller registration, hardware IRQ
//! to Linux virtual IRQ (virq) mapping, and IRQ allocation similar to
//! Linux's `kernel/irq/irqdomain.c`. Supports linear, tree, and nomap
//! domain types.

use alloc::collections::BTreeMap;
use alloc::string::String;
use core::sync::atomic::{AtomicU32, Ordering};
use spin::RwLock;

// ── Types ───────────────────────────────────────────────────────────────

/// IRQ domain type (Linux `enum irq_domain_bus_token`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IrqDomainType {
    /// Linear map (hwirq → virq direct array).
    Linear,
    /// Tree map (radix tree for sparse hwirqs).
    Tree,
    /// No map (hwirq == virq).
    Nomap,
}

/// IRQ trigger type (Linux `irqd_trigger_type` / IRQ_TYPE_*).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IrqTriggerType {
    None,
    EdgeRising,
    EdgeFalling,
    EdgeBoth,
    LevelHigh,
    LevelLow,
}

impl IrqTriggerType {
    pub fn flags(self) -> u32 {
        match self {
            IrqTriggerType::None => 0,
            IrqTriggerType::EdgeRising => 0x01,
            IrqTriggerType::EdgeFalling => 0x02,
            IrqTriggerType::EdgeBoth => 0x03,
            IrqTriggerType::LevelHigh => 0x04,
            IrqTriggerType::LevelLow => 0x08,
        }
    }
}

/// Operations implemented by an interrupt controller (Linux `struct irq_domain_ops`).
pub struct IrqDomainOps {
    pub activate: fn(hwirq: u32, trigger: IrqTriggerType) -> Result<(), &'static str>,
    pub deactivate: fn(hwirq: u32) -> Result<(), &'static str>,
    pub set_type: fn(hwirq: u32, trigger: IrqTriggerType) -> Result<(), &'static str>,
    pub get_name: fn() -> &'static str,
}

struct IrqDomain {
    id: u32,
    name: String,
    domain_type: IrqDomainType,
    hwirq_max: u32,
    ops: &'static IrqDomainOps,
    /// Mapping from hardware IRQ to virtual IRQ.
    hwirq_to_virq: BTreeMap<u32, u32>,
    /// Reverse mapping from virtual IRQ to hardware IRQ.
    virq_to_hwirq: BTreeMap<u32, u32>,
    /// Per-IRQ trigger type.
    irq_trigger: BTreeMap<u32, IrqTriggerType>,
    /// Per-IRQ handler data.
    irq_data: BTreeMap<u32, u64>,
    parent: Option<u32>,
}

// ── Registry ────────────────────────────────────────────────────────────

static IRQ_DOMAINS: RwLock<BTreeMap<u32, IrqDomain>> = RwLock::new(BTreeMap::new());
static NEXT_DOMAIN_ID: AtomicU32 = AtomicU32::new(0);
static NEXT_VIRQ: AtomicU32 = AtomicU32::new(32); // Start at 32 to avoid legacy IRQs

// ── Default ops for root interrupt controller ───────────────────────────

fn default_activate(_hwirq: u32, _trigger: IrqTriggerType) -> Result<(), &'static str> {
    Ok(())
}

fn default_deactivate(_hwirq: u32) -> Result<(), &'static str> {
    Ok(())
}

fn default_set_type(_hwirq: u32, _trigger: IrqTriggerType) -> Result<(), &'static str> {
    Ok(())
}

fn default_name() -> &'static str {
    "root"
}

pub static ROOT_DOMAIN_OPS: IrqDomainOps = IrqDomainOps {
    activate: default_activate,
    deactivate: default_deactivate,
    set_type: default_set_type,
    get_name: default_name,
};

// ── Public API ──────────────────────────────────────────────────────────

/// Create an IRQ domain (Linux irq_domain_create_*).
pub fn create_domain(
    name: &str,
    domain_type: IrqDomainType,
    hwirq_max: u32,
    ops: &'static IrqDomainOps,
    parent: Option<u32>,
) -> Result<u32, &'static str> {
    if let Some(pid) = parent {
        if !IRQ_DOMAINS.read().contains_key(&pid) {
            return Err("Parent IRQ domain not found");
        }
    }

    let id = NEXT_DOMAIN_ID.fetch_add(1, Ordering::SeqCst);
    IRQ_DOMAINS.write().insert(
        id,
        IrqDomain {
            id,
            name: String::from(name),
            domain_type,
            hwirq_max,
            ops,
            hwirq_to_virq: BTreeMap::new(),
            virq_to_hwirq: BTreeMap::new(),
            irq_trigger: BTreeMap::new(),
            irq_data: BTreeMap::new(),
            parent,
        },
    );
    Ok(id)
}

/// Create a mapping from hardware IRQ to virtual IRQ (Linux irq_create_mapping).
pub fn create_mapping(domain_id: u32, hwirq: u32) -> Result<u32, &'static str> {
    let (existing, ops, hwirq_max) = {
        let domains = IRQ_DOMAINS.read();
        let domain = domains.get(&domain_id).ok_or("IRQ domain not found")?;
        if hwirq > domain.hwirq_max {
            return Err("Hardware IRQ exceeds domain max");
        }
        (
            domain.hwirq_to_virq.get(&hwirq).copied(),
            domain.ops,
            domain.hwirq_max,
        )
    };

    // Return existing mapping if present.
    if let Some(virq) = existing {
        return Ok(virq);
    }

    // For nomap domains, hwirq == virq.
    let virq = match {
        let domains = IRQ_DOMAINS.read();
        domains.get(&domain_id).map(|d| d.domain_type)
    } {
        Some(IrqDomainType::Nomap) => hwirq,
        _ => NEXT_VIRQ.fetch_add(1, Ordering::SeqCst),
    };

    let mut domains = IRQ_DOMAINS.write();
    let domain = domains.get_mut(&domain_id).ok_or("IRQ domain vanished")?;
    domain.hwirq_to_virq.insert(hwirq, virq);
    domain.virq_to_hwirq.insert(virq, hwirq);
    domain.irq_trigger.insert(virq, IrqTriggerType::None);

    // Activate the IRQ on the controller.
    let _ = (ops.activate)(hwirq, IrqTriggerType::None);

    let _ = hwirq_max; // already validated above
    Ok(virq)
}

/// Find the virtual IRQ for a hardware IRQ (Linux irq_find_mapping).
pub fn find_mapping(domain_id: u32, hwirq: u32) -> Option<u32> {
    let domains = IRQ_DOMAINS.read();
    let domain = domains.get(&domain_id)?;
    domain.hwirq_to_virq.get(&hwirq).copied()
}

/// Set the trigger type for an IRQ (Linux irq_set_irq_type).
pub fn set_irq_type(virq: u32, trigger: IrqTriggerType) -> Result<(), &'static str> {
    let (domain_id, hwirq, set_type_fn) = {
        let domains = IRQ_DOMAINS.read();
        let mut found = None;
        for domain in domains.values() {
            if let Some(hw) = domain.virq_to_hwirq.get(&virq) {
                found = Some((domain.id, *hw, domain.ops.set_type));
                break;
            }
        }
        found.ok_or("Virtual IRQ not found in any domain")?
    };

    (set_type_fn)(hwirq, trigger)?;

    let mut domains = IRQ_DOMAINS.write();
    if let Some(domain) = domains.get_mut(&domain_id) {
        domain.irq_trigger.insert(virq, trigger);
    }
    Ok(())
}

/// Get the trigger type for an IRQ.
pub fn get_irq_type(virq: u32) -> Result<IrqTriggerType, &'static str> {
    let domains = IRQ_DOMAINS.read();
    for domain in domains.values() {
        if let Some(trigger) = domain.irq_trigger.get(&virq) {
            return Ok(*trigger);
        }
    }
    Err("Virtual IRQ not found")
}

/// Set per-IRQ handler data (Linux `irq_set_handler_data`).
pub fn set_irq_data(virq: u32, data: u64) -> Result<(), &'static str> {
    let mut domains = IRQ_DOMAINS.write();
    for domain in domains.values_mut() {
        if domain.virq_to_hwirq.contains_key(&virq) {
            domain.irq_data.insert(virq, data);
            return Ok(());
        }
    }
    Err("Virtual IRQ not found")
}

/// Get per-IRQ handler data.
pub fn get_irq_data(virq: u32) -> Result<u64, &'static str> {
    let domains = IRQ_DOMAINS.read();
    for domain in domains.values() {
        if let Some(data) = domain.irq_data.get(&virq) {
            return Ok(*data);
        }
    }
    Err("Virtual IRQ not found")
}

/// Resolve a virtual IRQ to its domain and hardware IRQ.
pub fn resolve_virq(virq: u32) -> Option<(u32, u32)> {
    let domains = IRQ_DOMAINS.read();
    for domain in domains.values() {
        if let Some(hwirq) = domain.virq_to_hwirq.get(&virq) {
            return Some((domain.id, *hwirq));
        }
    }
    None
}

/// Remove an IRQ mapping (Linux irq_dispose_mapping).
pub fn dispose_mapping(virq: u32) -> Result<(), &'static str> {
    let mut domains = IRQ_DOMAINS.write();
    for domain in domains.values_mut() {
        if let Some(hwirq) = domain.virq_to_hwirq.remove(&virq) {
            domain.hwirq_to_virq.remove(&hwirq);
            domain.irq_trigger.remove(&virq);
            domain.irq_data.remove(&virq);
            let _ = (domain.ops.deactivate)(hwirq);
            return Ok(());
        }
    }
    Err("Virtual IRQ not found")
}

/// Number of registered IRQ domains.
pub fn domain_count() -> usize {
    IRQ_DOMAINS.read().len()
}

/// Total number of mapped IRQs across all domains.
pub fn total_mappings() -> usize {
    IRQ_DOMAINS
        .read()
        .values()
        .map(|d| d.hwirq_to_virq.len())
        .sum()
}

/// Get domain name.
pub fn get_domain_name(domain_id: u32) -> Result<String, &'static str> {
    let domains = IRQ_DOMAINS.read();
    let domain = domains.get(&domain_id).ok_or("IRQ domain not found")?;
    Ok(domain.name.clone())
}

/// Initialize IRQ domain subsystem with a root domain.
pub fn init() -> Result<(), &'static str> {
    if !IRQ_DOMAINS.read().is_empty() {
        return Ok(());
    }

    // Create root interrupt controller domain covering legacy ISA IRQs (0-15).
    create_domain(
        "root-ic",
        IrqDomainType::Linear,
        255,
        &ROOT_DOMAIN_OPS,
        None,
    )?;

    crate::serial_println!(
        "irq_domain: root domain registered ({} domains)",
        domain_count()
    );
    Ok(())
}
