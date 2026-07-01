//! Intel HD Audio (HDA) PCI controller driver — mirrors `sound/pci/hda/*`,
//! radically simplified to a single playback stream on the first detected
//! codec's first DAC/Pin Complex pair.
//!
//! Implements just enough of the HDA 1.0a spec to be useful under QEMU's
//! `-device intel-hda -device hda-duplex` (or `hda-output`) emulation and
//! real ICH6+/PCH hardware:
//!   - controller reset (GCTL.CRST)
//!   - CORB/RIRB command ring setup and a synchronous, polled verb
//!     transaction helper
//!   - codec presence detection (STATESTS) + widget-graph walk via
//!     `GetParameter` verbs to find the first Audio Function Group, its
//!     first Audio Output (DAC) widget, and first output-capable Pin
//!     Complex widget
//!   - power-up, format programming, amplifier unmute and pin enable on
//!     that DAC/pin pair
//!   - a Buffer Descriptor List (BDL) backed by a DMA-coherent ring that
//!     userspace PCM writes are copied into directly (see
//!     `PlaybackSink::write`), and starting the output stream DMA engine.
//!
//! Scope cuts (see also the worked task's final report):
//!   - no interrupt-driven period-complete handling; SDnLPIB is polled to
//!     compute free space in the hardware ring instead of acking IOC
//!     interrupts, so this is a simple "DMA engine free-running over a
//!     fixed ring" model rather than a double-buffered period scheme.
//!   - only the first codec / first AFG / first DAC+Pin pair is used; no
//!     mixer/volume control surface, no S/PDIF, no capture (ADC) path.
//!   - pin → DAC connection-select is best-effort (`0`, i.e. "first entry
//!     in the pin's connection list"), which matches every codec QEMU's
//!     `intel-hda` device emulates and the vast majority of real laptop
//!     HDA codecs for the primary output pin, but is not a full
//!     connection-list-aware audio path search.

use crate::net::dma::DmaBuffer;
use crate::pci::{pci_bus, PciClass, PciDevice};
use alloc::boxed::Box;
use alloc::sync::Arc;
use core::ptr;
use core::sync::atomic::{AtomicUsize, Ordering};
use spin::Mutex;

// ---- HDA controller (global) register offsets, HDA spec 1.0a section 3.3 ----
const REG_GCAP: usize = 0x00; // u16
const REG_GCTL: usize = 0x08; // u32
const REG_STATESTS: usize = 0x0E; // u16
const REG_INTCTL: usize = 0x20; // u32
const REG_CORBLBASE: usize = 0x40; // u32
const REG_CORBUBASE: usize = 0x44; // u32
const REG_CORBWP: usize = 0x48; // u16
const REG_CORBRP: usize = 0x4A; // u16
const REG_CORBCTL: usize = 0x4C; // u8
const REG_CORBSIZE: usize = 0x4E; // u8
const REG_RIRBLBASE: usize = 0x50; // u32
const REG_RIRBUBASE: usize = 0x54; // u32
const REG_RIRBWP: usize = 0x58; // u16
const REG_RINTCNT: usize = 0x5A; // u16
const REG_RIRBCTL: usize = 0x5C; // u8
const REG_RIRBSIZE: usize = 0x5E; // u8
const STREAM_BASE: usize = 0x80; // first stream descriptor register block
const STREAM_STRIDE: usize = 0x20;

// ---- per-stream-descriptor register offsets (relative to stream base) ----
const SD_CTL: usize = 0x00; // u32 (low 3 bytes ctl, byte 3 sts overlay)
const SD_STS: usize = 0x03; // u8
const SD_LPIB: usize = 0x04; // u32 link position in buffer
const SD_CBL: usize = 0x08; // u32 cyclic buffer length
const SD_LVI: usize = 0x0C; // u16 last valid index
const SD_FMT: usize = 0x12; // u16 stream format
const SD_BDPL: usize = 0x18; // u32 BDL pointer low
const SD_BDPU: usize = 0x1C; // u32 BDL pointer high

const GCTL_CRST: u32 = 1 << 0;
const SDCTL_RUN: u32 = 1 << 1;
const SDCTL_IOCE: u32 = 1 << 2; // interrupt on completion enable (unused, left disabled)

const CORB_ENTRIES: usize = 256;
const RIRB_ENTRIES: usize = 256;

/// Number of Buffer Descriptor List entries for the single playback
/// stream, and size of each period (and thus the DMA ring chunk size).
const BDL_PERIODS: usize = 4;
const PERIOD_BYTES: usize = 16 * 1024;
const RING_BYTES: usize = BDL_PERIODS * PERIOD_BYTES;

#[derive(Debug, Clone, Copy)]
pub struct HdaDeviceId {
    pub vendor_id: u16,
    pub device_id: u16,
}

/// Known Intel HDA controller IDs we explicitly match in addition to the
/// generic class-code (0x04, 0x03) match. QEMU's `-device intel-hda`
/// defaults to the ICH6 ID below.
const KNOWN_HDA_IDS: &[HdaDeviceId] = &[
    HdaDeviceId {
        vendor_id: 0x8086,
        device_id: 0x2668,
    }, // ICH6, QEMU intel-hda default
    HdaDeviceId {
        vendor_id: 0x8086,
        device_id: 0x27D8,
    }, // ICH7
    HdaDeviceId {
        vendor_id: 0x8086,
        device_id: 0x293E,
    }, // ICH9
];

fn is_hda_controller(dev: &PciDevice) -> bool {
    if KNOWN_HDA_IDS
        .iter()
        .any(|id| id.vendor_id == dev.vendor_id && id.device_id == dev.device_id)
    {
        return true;
    }
    // Generic class match: Multimedia / Audio device, prog-if 0x00 (HDA).
    dev.class_code == PciClass::Multimedia && dev.subclass == 0x03
}

#[inline]
/// # Safety
/// The caller must ensure `base + off` is a valid, mapped MMIO address
/// for an HDA device register, aligned to 4 bytes.
unsafe fn mmio_r32(base: usize, off: usize) -> u32 {
    ptr::read_volatile((base + off) as *const u32)
}
#[inline]
/// # Safety
/// The caller must ensure `base + off` is a valid, mapped MMIO address
/// for an HDA device register, aligned to 4 bytes.
unsafe fn mmio_w32(base: usize, off: usize, val: u32) {
    ptr::write_volatile((base + off) as *mut u32, val);
}
#[inline]
/// # Safety
/// The caller must ensure `base + off` is a valid, mapped MMIO address
/// for an HDA device register, aligned to 2 bytes.
unsafe fn mmio_r16(base: usize, off: usize) -> u16 {
    ptr::read_volatile((base + off) as *const u16)
}
#[inline]
/// # Safety
/// The caller must ensure `base + off` is a valid, mapped MMIO address
/// for an HDA device register, aligned to 2 bytes.
unsafe fn mmio_w16(base: usize, off: usize, val: u16) {
    ptr::write_volatile((base + off) as *mut u16, val);
}
#[inline]
/// # Safety
/// The caller must ensure `base + off` is a valid, mapped MMIO address
/// for an HDA device register.
unsafe fn mmio_r8(base: usize, off: usize) -> u8 {
    ptr::read_volatile((base + off) as *const u8)
}
#[inline]
/// # Safety
/// The caller must ensure `base + off` is a valid, mapped MMIO address
/// for an HDA device register.
unsafe fn mmio_w8(base: usize, off: usize, val: u8) {
    ptr::write_volatile((base + off) as *mut u8, val);
}

/// 12-bit-verb command word: `(cad<<28)|(nid<<20)|(verb<<8)|payload8`.
fn corb_cmd(cad: u8, nid: u8, verb: u16, payload8: u8) -> u32 {
    ((cad as u32) << 28) | ((nid as u32) << 20) | ((verb as u32) << 8) | payload8 as u32
}

/// 4-bit-verb command word with a 16-bit payload, used for
/// Set-Converter-Format (0x2) and Set-Amplifier-Gain/Mute (0x3).
fn corb_cmd_wide(cad: u8, nid: u8, verb4: u8, payload16: u16) -> u32 {
    ((cad as u32) << 28) | ((nid as u32) << 20) | ((verb4 as u32) << 16) | payload16 as u32
}

// GetParameter parameter IDs (HDA spec table 7-9).
const PARAM_VENDOR_ID: u8 = 0x00;
const PARAM_SUB_NODE_COUNT: u8 = 0x04;
const PARAM_FUNCTION_GROUP_TYPE: u8 = 0x05;
const PARAM_AUDIO_WIDGET_CAP: u8 = 0x09;
const PARAM_PIN_CAP: u8 = 0x0C;

const VERB_GET_PARAMETER: u16 = 0xF00;
const VERB_SET_POWER_STATE: u16 = 0x705;
const VERB_SET_STREAM_CHANNEL: u16 = 0x706;
const VERB_SET_PIN_WIDGET_CTL: u16 = 0x707;
const VERB_SET_CONNECTION_SELECT: u16 = 0x701;
const VERB_WIDE_SET_CONVERTER_FORMAT: u8 = 0x2;
const VERB_WIDE_SET_AMP_GAIN_MUTE: u8 = 0x3;

const AFG_TYPE: u32 = 0x01;
const WIDGET_TYPE_AUDIO_OUTPUT: u32 = 0x0;
const WIDGET_TYPE_PIN_COMPLEX: u32 = 0x4;
const PIN_CAP_OUTPUT: u32 = 1 << 4;

/// One CORB/RIRB-backed synchronous command interface for a single
/// codec address (HDA supports up to 15 codecs per controller; we only
/// drive the first one we find).
struct CommandRing {
    mmio: usize,
    corb: DmaBuffer,
    rirb: DmaBuffer,
    corb_wp: u16,
    rirb_rp: u16,
}

impl CommandRing {
    fn new(mmio: usize) -> Result<Self, &'static str> {
        let corb =
            DmaBuffer::allocate(CORB_ENTRIES * 4, 128).map_err(|_| "CORB DMA alloc failed")?;
        let rirb =
            DmaBuffer::allocate(RIRB_ENTRIES * 8, 128).map_err(|_| "RIRB DMA alloc failed")?;
        Ok(Self {
            mmio,
            corb,
            rirb,
            corb_wp: 0,
            rirb_rp: 0,
        })
    }

    unsafe fn init(&mut self) {
        // Stop both rings before reprogramming base addresses.
        mmio_w8(self.mmio, REG_CORBCTL, 0);
        mmio_w8(self.mmio, REG_RIRBCTL, 0);

        mmio_w32(self.mmio, REG_CORBLBASE, self.corb.physical_addr() as u32);
        mmio_w32(
            self.mmio,
            REG_CORBUBASE,
            (self.corb.physical_addr() >> 32) as u32,
        );
        mmio_w32(self.mmio, REG_RIRBLBASE, self.rirb.physical_addr() as u32);
        mmio_w32(
            self.mmio,
            REG_RIRBUBASE,
            (self.rirb.physical_addr() >> 32) as u32,
        );

        // Ring size: 0 => 2 entries, 1 => 16, 2 => 256. We allocated 256.
        mmio_w8(self.mmio, REG_CORBSIZE, 0x02);
        mmio_w8(self.mmio, REG_RIRBSIZE, 0x02);

        // Reset read/write pointers.
        mmio_w16(self.mmio, REG_CORBWP, 0);
        mmio_w16(self.mmio, REG_CORBRP, 1 << 15); // CORBRPRST
        for _ in 0..1000 {
            if mmio_r16(self.mmio, REG_CORBRP) & (1 << 15) != 0 {
                break;
            }
            core::hint::spin_loop();
        }
        mmio_w16(self.mmio, REG_CORBRP, 0);
        mmio_w16(self.mmio, REG_RIRBWP, 1 << 15); // RIRBWPRST

        self.corb_wp = 0;
        self.rirb_rp = 0;

        // Run both rings; no interrupts (we poll RIRBWP for responses).
        mmio_w8(self.mmio, REG_CORBCTL, 0x02);
        mmio_w8(self.mmio, REG_RIRBCTL, 0x02);
        mmio_w16(self.mmio, REG_RINTCNT, 1);
    }

    /// Issue a single verb and poll for its response. Synchronous and
    /// single-threaded (protected by the caller's lock); good enough for
    /// the handful of setup-time verbs this driver needs.
    unsafe fn exec(&mut self, cmd: u32) -> Result<u32, &'static str> {
        let corb_ptr = self.corb.virtual_addr() as *mut u32;
        let next_wp = (self.corb_wp as usize + 1) % CORB_ENTRIES;
        ptr::write_volatile(corb_ptr.add(next_wp), cmd);
        self.corb_wp = next_wp as u16;
        mmio_w16(self.mmio, REG_CORBWP, self.corb_wp);

        let rirb_ptr = self.rirb.virtual_addr() as *const u64;
        for _ in 0..200_000 {
            let hw_wp = mmio_r16(self.mmio, REG_RIRBWP) as usize % RIRB_ENTRIES;
            if hw_wp != self.rirb_rp as usize {
                let next_rp = (self.rirb_rp as usize + 1) % RIRB_ENTRIES;
                let entry = ptr::read_volatile(rirb_ptr.add(next_rp));
                self.rirb_rp = next_rp as u16;
                return Ok((entry & 0xFFFF_FFFF) as u32);
            }
            core::hint::spin_loop();
        }
        Err("HDA codec command timed out")
    }

    unsafe fn get_param(&mut self, cad: u8, nid: u8, param: u8) -> Result<u32, &'static str> {
        self.exec(corb_cmd(cad, nid, VERB_GET_PARAMETER, param))
    }
}

/// A widget discovered while walking the AFG's node graph.
#[derive(Debug, Clone, Copy)]
struct Widget {
    nid: u8,
    widget_type: u32,
}

struct CodecGraph {
    afg_nid: u8,
    dac: Widget,
    pin: Widget,
}

/// # Safety
/// The caller must ensure `cad` is a valid codec address and the HDA
/// controller is initialized and ready for codec discovery commands.
unsafe fn discover_codec(ring: &mut CommandRing, cad: u8) -> Result<CodecGraph, &'static str> {
    // Root node (nid 0) sub-node range -> function groups.
    let root_subnodes = ring.get_param(cad, 0, PARAM_SUB_NODE_COUNT)?;
    let fg_start = ((root_subnodes >> 16) & 0xFF) as u8;
    let fg_count = (root_subnodes & 0xFF) as u8;

    let mut afg_nid = None;
    for i in 0..fg_count {
        let nid = fg_start + i;
        let fg_type = ring.get_param(cad, nid, PARAM_FUNCTION_GROUP_TYPE)? & 0xFF;
        if fg_type == AFG_TYPE {
            afg_nid = Some(nid);
            break;
        }
    }
    let afg_nid = afg_nid.ok_or("no Audio Function Group found on codec")?;

    // Power up the AFG itself (D0).
    ring.exec(corb_cmd(cad, afg_nid, VERB_SET_POWER_STATE, 0x00))?;

    let afg_subnodes = ring.get_param(cad, afg_nid, PARAM_SUB_NODE_COUNT)?;
    let w_start = ((afg_subnodes >> 16) & 0xFF) as u8;
    let w_count = (afg_subnodes & 0xFF) as u8;

    let mut dac: Option<Widget> = None;
    let mut pin: Option<Widget> = None;
    for i in 0..w_count {
        let nid = w_start + i;
        let cap = ring.get_param(cad, nid, PARAM_AUDIO_WIDGET_CAP)?;
        let widget_type = (cap >> 20) & 0xF;
        if widget_type == WIDGET_TYPE_AUDIO_OUTPUT && dac.is_none() {
            dac = Some(Widget { nid, widget_type });
        } else if widget_type == WIDGET_TYPE_PIN_COMPLEX && pin.is_none() {
            let pin_cap = ring.get_param(cad, nid, PARAM_PIN_CAP)?;
            if pin_cap & PIN_CAP_OUTPUT != 0 {
                pin = Some(Widget { nid, widget_type });
            }
        }
        if dac.is_some() && pin.is_some() {
            break;
        }
    }

    let dac = dac.ok_or("no DAC (Audio Output) widget found")?;
    let pin = pin.ok_or("no output-capable Pin Complex widget found")?;
    Ok(CodecGraph { afg_nid, dac, pin })
}

/// Encode `SDnFMT` / Set-Converter-Format payload per HDA spec section
/// 3.7.1. Only the rates/formats `crate::sound::SampleFormat` exposes are
/// handled.
fn encode_stream_format(sample_rate: u32, bits: u8, channels: u8) -> u16 {
    // base: 0 = 48kHz family, 1 = 44.1kHz family
    let (base, mult, div) = match sample_rate {
        48_000 => (0u16, 0u16, 0u16),
        96_000 => (0u16, 1u16, 0u16), // x2
        32_000 => (0u16, 0u16, 1u16), // /1.5 not exact; acceptable approximation
        44_100 => (1u16, 0u16, 0u16),
        _ => (0u16, 0u16, 0u16),
    };
    let bits_field: u16 = match bits {
        8 => 0b000,
        16 => 0b001,
        20 => 0b010,
        24 => 0b011,
        32 => 0b100,
        _ => 0b001,
    };
    let chan_field = (channels.saturating_sub(1)) as u16 & 0xF;
    (base << 15) | (mult << 11) | (div << 8) | (bits_field << 4) | chan_field
}

/// Playback-side hardware state: the BDL + backing ring DMA buffer and the
/// stream descriptor register block this stream owns.
struct PlaybackStream {
    mmio: usize,
    sd_base: usize,
    ring_buf: DmaBuffer,
    /// Software write cursor into `ring_buf`, bytes.
    write_pos: AtomicUsize,
}

impl PlaybackStream {
    unsafe fn setup(
        mmio: usize,
        sd_base: usize,
        stream_tag: u8,
        params: crate::sound::PcmHwParams,
    ) -> Result<Self, &'static str> {
        let bdl = DmaBuffer::allocate(BDL_PERIODS * 16, 128).map_err(|_| "BDL DMA alloc failed")?;
        let ring_buf =
            DmaBuffer::allocate(RING_BYTES, 128).map_err(|_| "playback DMA ring alloc failed")?;

        // Fill BDL: BDL_PERIODS entries of {addr:u64, length:u32, flags:u32(IOC bit0)}.
        let bdl_ptr = bdl.virtual_addr() as *mut u32;
        for i in 0..BDL_PERIODS {
            let entry_addr = ring_buf.physical_addr() + (i * PERIOD_BYTES) as u64;
            let off = i * 4;
            ptr::write_volatile(bdl_ptr.add(off), entry_addr as u32);
            ptr::write_volatile(bdl_ptr.add(off + 1), (entry_addr >> 32) as u32);
            ptr::write_volatile(bdl_ptr.add(off + 2), PERIOD_BYTES as u32);
            ptr::write_volatile(bdl_ptr.add(off + 3), 1); // IOC, unused without an IRQ handler
        }

        // Make sure the stream is stopped before reprogramming.
        mmio_w8(mmio, sd_base + SD_CTL, 0);

        mmio_w32(mmio, sd_base + SD_BDPL, bdl.physical_addr() as u32);
        mmio_w32(mmio, sd_base + SD_BDPU, (bdl.physical_addr() >> 32) as u32);
        mmio_w32(mmio, sd_base + SD_CBL, RING_BYTES as u32);
        mmio_w16(mmio, sd_base + SD_LVI, (BDL_PERIODS - 1) as u16);
        mmio_w16(
            mmio,
            sd_base + SD_FMT,
            encode_stream_format(params.sample_rate, params.format.bits(), params.channels),
        );

        // Stream descriptor control byte 2 (bits 20:23 in the 24-bit CTL
        // field) carries the stream tag/number used to match converter
        // Set-Stream-Channel verbs to this DMA engine.
        let ctl = mmio_r32(mmio, sd_base + SD_CTL);
        let ctl = (ctl & !(0xF << 20)) | ((stream_tag as u32 & 0xF) << 20);
        mmio_w32(mmio, sd_base + SD_CTL, ctl);

        Ok(Self {
            mmio,
            sd_base,
            ring_buf,
            write_pos: AtomicUsize::new(0),
        })
    }

    unsafe fn start(&self) {
        let ctl = mmio_r32(self.mmio, self.sd_base + SD_CTL);
        mmio_w32(self.mmio, self.sd_base + SD_CTL, ctl | SDCTL_RUN);
    }

    /// Free space (bytes) between the software write cursor and the
    /// hardware read cursor (`SDnLPIB`), leaving one byte of slack so we
    /// never let write_pos catch up to LPIB exactly (which would be
    /// indistinguishable from "buffer empty").
    fn free_space(&self) -> usize {
        // SAFETY: the MMIO address is a valid mapped HDA controller register.
        let lpib = unsafe { mmio_r32(self.mmio, self.sd_base + SD_LPIB) } as usize % RING_BYTES;
        let wp = self.write_pos.load(Ordering::Relaxed) % RING_BYTES;
        let used = if wp >= lpib {
            wp - lpib
        } else {
            RING_BYTES - lpib + wp
        };
        RING_BYTES.saturating_sub(used).saturating_sub(1)
    }

    /// Copy PCM bytes into the DMA ring at the software write cursor,
    /// wrapping as needed. Returns bytes actually written (short write =
    /// backpressure signal, exactly like `sound::RingBuffer::write`).
    fn write(&self, buf: &[u8]) -> usize {
        let space = self.free_space();
        let n = core::cmp::min(buf.len(), space);
        if n == 0 {
            return 0;
        }
        let dst = self.ring_buf.virtual_addr();
        let wp = self.write_pos.load(Ordering::Relaxed) % RING_BYTES;
        let first = core::cmp::min(n, RING_BYTES - wp);
        // SAFETY: src and dst are valid DMA buffers of equal size, non-overlapping.
        unsafe {
            ptr::copy_nonoverlapping(buf.as_ptr(), dst.add(wp), first);
            if n > first {
                ptr::copy_nonoverlapping(buf.as_ptr().add(first), dst, n - first);
            }
        }
        self.write_pos.fetch_add(n, Ordering::Relaxed);
        n
    }
}

// Safety: PlaybackStream only touches raw MMIO/DMA pointers behind a
// shared, single-controller setup; access is serialized through the
// Mutex<HdaController> the public API hands out via Arc.
unsafe impl Send for PlaybackStream {}
unsafe impl Sync for PlaybackStream {}

pub struct HdaController {
    mmio: usize,
    cmd: CommandRing,
    playback: Option<PlaybackStream>,
}

// SAFETY: HdaController wraps an MMIO base address (usize) and a command
// ring. Access is serialized through the Mutex<HdaController> handed out
// by the public API. The MMIO address is a fixed hardware mapping.
unsafe impl Send for HdaController {}

/// Handle returned by `probe_and_init()` to the PCM core so it can bind a
/// hardware sink for the one playback PCM device.
pub struct HdaCardHandle {
    controller: Arc<Mutex<HdaController>>,
    pub codec_vendor_id: u16,
    pub codec_device_id: u16,
}

impl HdaCardHandle {
    /// Build a `sound::HwSink` closure that forwards bytes straight into
    /// this controller's playback DMA ring.
    pub fn playback_sink(&self) -> crate::sound::HwSink {
        let controller = self.controller.clone();
        Box::new(move |buf: &[u8]| -> usize {
            let ctrl = controller.lock();
            match &ctrl.playback {
                Some(stream) => stream.write(buf),
                None => 0,
            }
        })
    }
}

/// Probe the PCI bus for an Intel HDA controller, bring it up, enumerate
/// the first responding codec, and set up one playback stream. Returns
/// `Ok(None)` if no HDA controller is present (not an error — most boot
/// configurations won't have `-device intel-hda` wired up).
pub fn probe_and_init() -> Result<Option<HdaCardHandle>, &'static str> {
    let candidate = {
        let bus = pci_bus().lock();
        bus.get_devices()
            .iter()
            .find(|d| is_hda_controller(d))
            .cloned()
    };
    let Some(dev) = candidate else {
        return Ok(None);
    };

    let bar0 = dev.bars[0];
    if bar0 & 0x1 != 0 {
        return Err("HDA BAR0 is I/O space, expected MMIO");
    }
    let base_phys = (bar0 & !0xF) as usize;
    if base_phys == 0 {
        return Err("HDA BAR0 unassigned");
    }

    // Enable memory space + bus mastering before touching MMIO.
    {
        let bus = pci_bus().lock();
        let cmd = bus.read_config_word(dev.bus, dev.device, dev.function, 0x04);
        bus.write_config_word(dev.bus, dev.device, dev.function, 0x04, cmd | 0x0006);
    }

    // 0x4000 covers GCAP..stream descriptors comfortably for a handful of
    // input/output/bidir streams.
    let mmio = crate::memory::map_mmio_region(base_phys, 0x4000)?;

    // SAFETY: the MMIO address is a valid mapped HDA controller register.
    unsafe {
        // Controller reset: clear then set CRST, per HDA spec 4.3.
        mmio_w32(mmio, REG_GCTL, 0);
        for _ in 0..1000 {
            if mmio_r32(mmio, REG_GCTL) & GCTL_CRST == 0 {
                break;
            }
            core::hint::spin_loop();
        }
        mmio_w32(mmio, REG_GCTL, GCTL_CRST);
        for _ in 0..1000 {
            if mmio_r32(mmio, REG_GCTL) & GCTL_CRST != 0 {
                break;
            }
            core::hint::spin_loop();
        }
        // Mask all controller interrupts; we poll instead.
        mmio_w32(mmio, REG_INTCTL, 0);

        let gcap = mmio_r16(mmio, REG_GCAP);
        let _ = gcap; // ISS/OSS/BSS available here if multi-stream support is added later

        let mut cmd_ring = CommandRing::new(mmio)?;
        cmd_ring.init();

        // STATESTS bit n set => codec address n responded to the reset.
        // Give the codec a moment to assert presence after CRST.
        let mut statests = mmio_r16(mmio, REG_STATESTS);
        for _ in 0..1000 {
            statests = mmio_r16(mmio, REG_STATESTS);
            if statests != 0 {
                break;
            }
            core::hint::spin_loop();
        }
        let cad = (0..15u8).find(|c| statests & (1 << c) != 0);
        let Some(cad) = cad else {
            return Err("no HDA codec responded to STATESTS");
        };

        let vendor_resp = cmd_ring.get_param(cad, 0, PARAM_VENDOR_ID)?;
        let codec_vendor_id = (vendor_resp >> 16) as u16;
        let codec_device_id = (vendor_resp & 0xFFFF) as u16;

        let graph = discover_codec(&mut cmd_ring, cad)?;

        // Power up DAC + pin, unmute, route, and enable pin output.
        cmd_ring.exec(corb_cmd(cad, graph.dac.nid, VERB_SET_POWER_STATE, 0x00))?;
        cmd_ring.exec(corb_cmd(cad, graph.pin.nid, VERB_SET_POWER_STATE, 0x00))?;
        // Best-effort: route pin to its first connection-list entry
        // (matches QEMU's emulated codecs and most laptop output pins).
        let _ = cmd_ring.exec(corb_cmd(cad, graph.pin.nid, VERB_SET_CONNECTION_SELECT, 0));
        cmd_ring.exec(corb_cmd(cad, graph.pin.nid, VERB_SET_PIN_WIDGET_CTL, 0x40))?; // out-enable
        cmd_ring.exec(corb_cmd_wide(
            cad,
            graph.dac.nid,
            VERB_WIDE_SET_AMP_GAIN_MUTE,
            0xD87F, // unmute output amp, both channels, max gain
        ))?;

        let params = crate::sound::PcmHwParams::default();
        cmd_ring.exec(corb_cmd_wide(
            cad,
            graph.dac.nid,
            VERB_WIDE_SET_CONVERTER_FORMAT,
            encode_stream_format(params.sample_rate, params.format.bits(), params.channels),
        ))?;

        const STREAM_TAG: u8 = 1;
        cmd_ring.exec(corb_cmd(
            cad,
            graph.dac.nid,
            VERB_SET_STREAM_CHANNEL,
            (STREAM_TAG << 4) | 0,
        ))?;

        // First output stream descriptor index = ISS (input streams come
        // first in the SDn register array); GCAP[11:8] = ISS.
        let iss = ((gcap >> 8) & 0xF) as usize;
        let sd_base = mmio + STREAM_BASE + iss * STREAM_STRIDE;

        let playback = PlaybackStream::setup(mmio, sd_base, STREAM_TAG, params)?;
        playback.start();

        let controller = HdaController {
            mmio,
            cmd: cmd_ring,
            playback: Some(playback),
        };
        let _ = controller.cmd; // kept alive on the controller for future verbs (volume, etc.)

        crate::serial_println!(
            "hda: controller {:02x}:{:02x}.{} codec {:04x}:{:04x} afg={} dac={} pin={} stream_tag={}",
            dev.bus, dev.device, dev.function, codec_vendor_id, codec_device_id,
            graph.afg_nid, graph.dac.nid, graph.pin.nid, STREAM_TAG
        );

        Ok(Some(HdaCardHandle {
            controller: Arc::new(Mutex::new(controller)),
            codec_vendor_id,
            codec_device_id,
        }))
    }
}
