//! LED class subsystem
//!
//! Provides LED device registration, brightness control, and trigger
//! binding similar to Linux's LED class (`drivers/leds/led-class.c`).
//! Includes keyboard LED (caps lock, num lock, scroll lock) support
//! via PS/2 controller port 0x60/0x64.

use alloc::collections::BTreeMap;
use alloc::string::String;
use core::sync::atomic::{AtomicU32, AtomicU8, Ordering};
use spin::RwLock;

// ── Types ───────────────────────────────────────────────────────────────

/// LED brightness (0 = off, 255 = full brightness).
pub type LedBrightness = u8;

/// LED trigger patterns (Linux ledtrig-* equivalents).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LedTrigger {
    None,
    Heartbeat,
    Timer,
    Oneshot,
    DiskActivity,
    CpuActivity,
    KeyboardCapsLock,
    KeyboardNumLock,
    KeyboardScrollLock,
}

/// Operations implemented by an LED device driver.
#[derive(Clone, Copy)]
pub struct LedOps {
    pub set_brightness: fn(LedBrightness) -> Result<(), &'static str>,
    pub get_brightness: fn() -> LedBrightness,
    pub max_brightness: fn() -> LedBrightness,
    pub get_name: fn() -> &'static str,
}

struct LedDevice {
    id: u32,
    name: String,
    ops: LedOps,
    current_brightness: LedBrightness,
    trigger: LedTrigger,
    blink_delay_on_ms: u32,
    blink_delay_off_ms: u32,
}

// ── Keyboard LED support (PS/2) ─────────────────────────────────────────

/// PS/2 keyboard LED bits.
const KBD_LED_SCROLL_LOCK: u8 = 0x01;
const KBD_LED_NUM_LOCK: u8 = 0x02;
const KBD_LED_CAPS_LOCK: u8 = 0x04;

static KBD_LED_STATE: AtomicU8 = AtomicU8::new(0);

/// Write keyboard LED state via PS/2 controller.
/// SAFETY: Performs raw I/O to PS/2 controller ports 0x60/0x64.
/// See docs/SAFETY.md#io-port-access.
unsafe fn write_kbd_leds(led_state: u8) {
    use core::arch::asm;
    // Wait for input buffer to be clear.
    let mut timeout = 0u32;
    loop {
        let status: u8;
        unsafe {
            asm!("in al, dx", in("dx") 0x64u16, out("al") status, options(nomem, nostack, preserves_flags));
        }
        if status & 0x02 == 0 {
            break;
        }
        timeout += 1;
        if timeout > 100_000 {
            return;
        }
        core::hint::spin_loop();
    }
    // Send Set LEDs command.
    unsafe {
        asm!("out dx, al", in("dx") 0x60u16, in("al") 0xEDu8, options(nomem, nostack, preserves_flags));
    }
    // Wait for ACK.
    timeout = 0;
    loop {
        let status: u8;
        unsafe {
            asm!("in al, dx", in("dx") 0x64u16, out("al") status, options(nomem, nostack, preserves_flags));
        }
        if status & 0x01 != 0 {
            let _ack: u8;
            unsafe {
                asm!("in al, dx", in("dx") 0x60u16, out("al") _ack, options(nomem, nostack, preserves_flags));
            }
            break;
        }
        timeout += 1;
        if timeout > 100_000 {
            return;
        }
        core::hint::spin_loop();
    }
    // Wait for input buffer clear again.
    timeout = 0;
    loop {
        let status: u8;
        unsafe {
            asm!("in al, dx", in("dx") 0x64u16, out("al") status, options(nomem, nostack, preserves_flags));
        }
        if status & 0x02 == 0 {
            break;
        }
        timeout += 1;
        if timeout > 100_000 {
            return;
        }
        core::hint::spin_loop();
    }
    // Send LED state byte.
    unsafe {
        asm!("out dx, al", in("dx") 0x60u16, in("al") led_state, options(nomem, nostack, preserves_flags));
    }
}

fn update_kbd_leds() {
    let state = KBD_LED_STATE.load(Ordering::Relaxed);
    unsafe { write_kbd_leds(state) };
}

// ── Individual keyboard LED drivers ─────────────────────────────────────

fn caps_lock_set(brightness: LedBrightness) -> Result<(), &'static str> {
    if brightness > 0 {
        KBD_LED_STATE.fetch_or(KBD_LED_CAPS_LOCK, Ordering::Relaxed);
    } else {
        KBD_LED_STATE.fetch_and(!KBD_LED_CAPS_LOCK, Ordering::Relaxed);
    }
    update_kbd_leds();
    Ok(())
}

fn caps_lock_get() -> LedBrightness {
    let state = KBD_LED_STATE.load(Ordering::Relaxed);
    if state & KBD_LED_CAPS_LOCK != 0 {
        255
    } else {
        0
    }
}

fn num_lock_set(brightness: LedBrightness) -> Result<(), &'static str> {
    if brightness > 0 {
        KBD_LED_STATE.fetch_or(KBD_LED_NUM_LOCK, Ordering::Relaxed);
    } else {
        KBD_LED_STATE.fetch_and(!KBD_LED_NUM_LOCK, Ordering::Relaxed);
    }
    update_kbd_leds();
    Ok(())
}

fn num_lock_get() -> LedBrightness {
    let state = KBD_LED_STATE.load(Ordering::Relaxed);
    if state & KBD_LED_NUM_LOCK != 0 {
        255
    } else {
        0
    }
}

fn scroll_lock_set(brightness: LedBrightness) -> Result<(), &'static str> {
    if brightness > 0 {
        KBD_LED_STATE.fetch_or(KBD_LED_SCROLL_LOCK, Ordering::Relaxed);
    } else {
        KBD_LED_STATE.fetch_and(!KBD_LED_SCROLL_LOCK, Ordering::Relaxed);
    }
    update_kbd_leds();
    Ok(())
}

fn scroll_lock_get() -> LedBrightness {
    let state = KBD_LED_STATE.load(Ordering::Relaxed);
    if state & KBD_LED_SCROLL_LOCK != 0 {
        255
    } else {
        0
    }
}

fn max_brightness_255() -> LedBrightness {
    255
}

const CAPS_LOCK_OPS: LedOps = LedOps {
    set_brightness: caps_lock_set,
    get_brightness: caps_lock_get,
    max_brightness: max_brightness_255,
    get_name: || "caps-lock",
};

const NUM_LOCK_OPS: LedOps = LedOps {
    set_brightness: num_lock_set,
    get_brightness: num_lock_get,
    max_brightness: max_brightness_255,
    get_name: || "num-lock",
};

const SCROLL_LOCK_OPS: LedOps = LedOps {
    set_brightness: scroll_lock_set,
    get_brightness: scroll_lock_get,
    max_brightness: max_brightness_255,
    get_name: || "scroll-lock",
};

// ── Registry ────────────────────────────────────────────────────────────

static LED_DEVICES: RwLock<BTreeMap<u32, LedDevice>> = RwLock::new(BTreeMap::new());
static NEXT_LED_ID: AtomicU32 = AtomicU32::new(0);

// ── Public API ──────────────────────────────────────────────────────────

/// Register an LED device (Linux led_classdev_register).
pub fn register_led(name: &str, ops: LedOps) -> Result<u32, &'static str> {
    let id = NEXT_LED_ID.fetch_add(1, Ordering::SeqCst);
    LED_DEVICES.write().insert(
        id,
        LedDevice {
            id,
            name: String::from(name),
            ops,
            current_brightness: 0,
            trigger: LedTrigger::None,
            blink_delay_on_ms: 0,
            blink_delay_off_ms: 0,
        },
    );
    Ok(id)
}

/// Set LED brightness (Linux led_set_brightness).
pub fn set_brightness(led_id: u32, brightness: LedBrightness) -> Result<(), &'static str> {
    let max = {
        let devices = LED_DEVICES.read();
        let dev = devices.get(&led_id).ok_or("LED device not found")?;
        (dev.ops.max_brightness)()
    };
    let clamped = if brightness > max { max } else { brightness };
    let set_brightness_fn = {
        let mut devices = LED_DEVICES.write();
        let dev = devices.get_mut(&led_id).ok_or("LED device not found")?;
        dev.current_brightness = clamped;
        dev.ops.set_brightness
    };
    (set_brightness_fn)(clamped)
}

/// Get current LED brightness (Linux led_get_brightness).
pub fn get_brightness(led_id: u32) -> Result<LedBrightness, &'static str> {
    let devices = LED_DEVICES.read();
    let dev = devices.get(&led_id).ok_or("LED device not found")?;
    Ok((dev.ops.get_brightness)())
}

/// Set LED trigger (Linux led_trigger_set).
pub fn set_trigger(led_id: u32, trigger: LedTrigger) -> Result<(), &'static str> {
    let mut devices = LED_DEVICES.write();
    let dev = devices.get_mut(&led_id).ok_or("LED device not found")?;
    dev.trigger = trigger;
    Ok(())
}

/// Get LED trigger.
pub fn get_trigger(led_id: u32) -> Result<LedTrigger, &'static str> {
    let devices = LED_DEVICES.read();
    let dev = devices.get(&led_id).ok_or("LED device not found")?;
    Ok(dev.trigger)
}

/// Configure blink parameters (Linux led_blink_set).
pub fn set_blink(led_id: u32, delay_on_ms: u32, delay_off_ms: u32) -> Result<(), &'static str> {
    let mut devices = LED_DEVICES.write();
    let dev = devices.get_mut(&led_id).ok_or("LED device not found")?;
    dev.blink_delay_on_ms = delay_on_ms;
    dev.blink_delay_off_ms = delay_off_ms;
    Ok(())
}

/// Toggle an LED on/off.
pub fn toggle(led_id: u32) -> Result<(), &'static str> {
    let current = get_brightness(led_id)?;
    let new = if current > 0 { 0 } else { 255 };
    set_brightness(led_id, new)
}

/// Number of registered LED devices.
pub fn led_count() -> usize {
    LED_DEVICES.read().len()
}

/// Find an LED by name.
pub fn find_by_name(name: &str) -> Option<u32> {
    LED_DEVICES
        .read()
        .iter()
        .find(|(_, dev)| dev.name == name)
        .map(|(id, _)| *id)
}

/// Initialize LED subsystem with keyboard LEDs.
pub fn init() -> Result<(), &'static str> {
    if !LED_DEVICES.read().is_empty() {
        return Ok(());
    }

    register_led("caps-lock", CAPS_LOCK_OPS)?;
    register_led("num-lock", NUM_LOCK_OPS)?;
    register_led("scroll-lock", SCROLL_LOCK_OPS)?;

    crate::serial_println!("leds: {} device(s) registered", led_count());
    Ok(())
}
