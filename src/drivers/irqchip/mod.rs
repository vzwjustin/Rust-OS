//! IRQ chip driver subsystem
//!
//! Provides interrupt controller (IRQ chip) registration and management.
//! Mirrors Linux's `drivers/irqchip/`.

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, Ordering};
use spin::RwLock;

// ── Types ───────────────────────────────────────────────────────────────

/// IRQ chip (Linux `struct irq_chip`).
pub struct IrqChip {
    pub id: u32,
    pub name: String,
    pub ops: IrqChipOps,
    pub parent_id: Option<u32>,
    pub irq_base: u32,
    pub irq_count: u32,
}

/// IRQ chip operations (Linux `struct irq_chip`).
pub struct IrqChipOps {
    pub ack: fn(irq: u32),
    pub mask: fn(irq: u32),
    pub unmask: fn(irq: u32),
    pub eoi: fn(irq: u32),
    pub set_affinity: Option<fn(irq: u32, cpu: u32) -> Result<(), &'static str>>,
    pub set_type: Option<fn(irq: u32, trigger: IrqTrigger) -> Result<(), &'static str>>,
    pub set_wake: Option<fn(irq: u32, enable: bool) -> Result<(), &'static str>>,
}

/// IRQ trigger type (Linux `enum irq_type`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IrqTrigger {
    EdgeRising,
    EdgeFalling,
    EdgeBoth,
    LevelHigh,
    LevelLow,
}

/// IRQ domain (Linux `struct irq_domain`).
pub struct IrqDomain {
    pub id: u32,
    pub name: String,
    pub chip_id: u32,
    pub hwirq_base: u32,
    pub hwirq_count: u32,
}

// ── Registry ────────────────────────────────────────────────────────────

static CHIP_ID_COUNTER: AtomicU32 = AtomicU32::new(0);
static DOMAIN_ID_COUNTER: AtomicU32 = AtomicU32::new(0);

static IRQ_CHIPS: RwLock<BTreeMap<u32, IrqChip>> = RwLock::new(BTreeMap::new());
static IRQ_DOMAINS: RwLock<BTreeMap<u32, IrqDomain>> = RwLock::new(BTreeMap::new());

// ── Public API ──────────────────────────────────────────────────────────

/// Register an IRQ chip (Linux `irq_set_chip`).
pub fn register_chip(
    name: &str,
    ops: IrqChipOps,
    parent_id: Option<u32>,
    irq_base: u32,
    irq_count: u32,
) -> Result<u32, &'static str> {
    let id = CHIP_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
    let chip = IrqChip {
        id,
        name: String::from(name),
        ops,
        parent_id,
        irq_base,
        irq_count,
    };
    IRQ_CHIPS.write().insert(id, chip);
    Ok(id)
}

/// Register an IRQ domain (Linux `irq_domain_add_*`).
pub fn register_domain(
    name: &str,
    chip_id: u32,
    hwirq_base: u32,
    hwirq_count: u32,
) -> Result<u32, &'static str> {
    if !IRQ_CHIPS.read().contains_key(&chip_id) {
        return Err("IRQ chip not found");
    }
    let id = DOMAIN_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
    let domain = IrqDomain {
        id,
        name: String::from(name),
        chip_id,
        hwirq_base,
        hwirq_count,
    };
    IRQ_DOMAINS.write().insert(id, domain);
    Ok(id)
}

/// Acknowledge an IRQ (Linux `irq_chip_ack`).
pub fn ack_irq(irq: u32) {
    if let Some(chip) = find_chip_for_irq(irq) {
        (chip.ops.ack)(irq);
    }
}

/// Mask an IRQ (Linux `irq_chip_mask`).
pub fn mask_irq(irq: u32) {
    if let Some(chip) = find_chip_for_irq(irq) {
        (chip.ops.mask)(irq);
    }
}

/// Unmask an IRQ (Linux `irq_chip_unmask`).
pub fn unmask_irq(irq: u32) {
    if let Some(chip) = find_chip_for_irq(irq) {
        (chip.ops.unmask)(irq);
    }
}

/// End-of-interrupt for an IRQ (Linux `irq_chip_eoi`).
pub fn eoi_irq(irq: u32) {
    if let Some(chip) = find_chip_for_irq(irq) {
        (chip.ops.eoi)(irq);
    }
}

/// Set CPU affinity for an IRQ.
pub fn set_affinity(irq: u32, cpu: u32) -> Result<(), &'static str> {
    let chip = find_chip_for_irq(irq).ok_or("No IRQ chip for this IRQ")?;
    if let Some(set_aff_fn) = chip.ops.set_affinity {
        (set_aff_fn)(irq, cpu)
    } else {
        Err("IRQ chip does not support set_affinity")
    }
}

/// Set trigger type for an IRQ.
pub fn set_trigger_type(irq: u32, trigger: IrqTrigger) -> Result<(), &'static str> {
    let chip = find_chip_for_irq(irq).ok_or("No IRQ chip for this IRQ")?;
    if let Some(set_type_fn) = chip.ops.set_type {
        (set_type_fn)(irq, trigger)
    } else {
        Err("IRQ chip does not support set_type")
    }
}

fn find_chip_for_irq(irq: u32) -> Option<IrqChip> {
    let chips = IRQ_CHIPS.read();
    chips
        .values()
        .find(|c| irq >= c.irq_base && irq < c.irq_base + c.irq_count)
        .map(|c| IrqChip {
            id: c.id,
            name: c.name.clone(),
            ops: IrqChipOps {
                ack: c.ops.ack,
                mask: c.ops.mask,
                unmask: c.ops.unmask,
                eoi: c.ops.eoi,
                set_affinity: c.ops.set_affinity,
                set_type: c.ops.set_type,
                set_wake: c.ops.set_wake,
            },
            parent_id: c.parent_id,
            irq_base: c.irq_base,
            irq_count: c.irq_count,
        })
}

/// List all IRQ chips.
pub fn list_chips() -> Vec<(u32, String, u32, u32)> {
    IRQ_CHIPS
        .read()
        .iter()
        .map(|(id, c)| (*id, c.name.clone(), c.irq_base, c.irq_count))
        .collect()
}

/// Count registered chips.
pub fn chip_count() -> usize {
    IRQ_CHIPS.read().len()
}

// ── Software IRQ chip ───────────────────────────────────────────────────

fn sw_ack(_irq: u32) {}
fn sw_mask(_irq: u32) {}
fn sw_unmask(_irq: u32) {}
fn sw_eoi(_irq: u32) {}

/// Software IRQ chip ops.
pub fn software_irqchip_ops() -> IrqChipOps {
    IrqChipOps {
        ack: sw_ack,
        mask: sw_mask,
        unmask: sw_unmask,
        eoi: sw_eoi,
        set_affinity: None,
        set_type: None,
        set_wake: None,
    }
}

// ── Init ────────────────────────────────────────────────────────────────

pub fn init() -> Result<(), &'static str> {
    if !IRQ_CHIPS.read().is_empty() {
        return Ok(());
    }

    let ops = software_irqchip_ops();
    let chip_id = register_chip("sw-ioapic", ops, None, 16, 224)?;
    register_domain("IO-APIC", chip_id, 0, 224)?;

    crate::serial_println!(
        "irqchip: software IO-APIC registered (chip_id={}, irqs 16-239)",
        chip_id
    );
    Ok(())
}
