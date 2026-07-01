//! ALSA-style PCM device registry and /dev/snd/* devfs integration.
//!
//! Mirrors a drastically simplified `sound/core/pcm*.c`: a small set of
//! `SoundCard`s own `PcmDevice`s, each backed by a byte-oriented circular
//! ring buffer with real backpressure (writes past capacity are short
//! writes, never silent data loss). A `PcmDevice` may additionally be
//! bound to a hardware sink (see `bind_hw_sink`) so that bytes written by
//! userspace are forwarded straight into a real DMA-backed playback
//! buffer owned by a PCI controller driver (e.g. `drivers::sound::hda`)
//! instead of just being held in software.

use alloc::boxed::Box;
use alloc::collections::BTreeMap;
use alloc::format;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, Ordering};
use spin::RwLock;

pub const ALSA_MAJOR: u32 = 116;

/// Default software ring buffer size in bytes when no hardware parameters
/// have been negotiated yet (~46ms at 44.1kHz/16bit/stereo).
const DEFAULT_RING_BYTES: usize = 8192;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PcmStream {
    Playback,
    Capture,
}

/// Simplified PCM sample format. Only the formats RustOS's HDA backend can
/// actually program into `SDnFMT` are modeled; this is intentionally not a
/// full ALSA `snd_pcm_format_t` enumeration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SampleFormat {
    S16Le,
}

impl SampleFormat {
    pub fn bits(self) -> u8 {
        match self {
            SampleFormat::S16Le => 16,
        }
    }

    pub fn bytes(self) -> usize {
        (self.bits() as usize) / 8
    }
}

/// Negotiated hardware parameters for a PCM substream, analogous to a
/// simplified `snd_pcm_hw_params`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PcmHwParams {
    pub sample_rate: u32,
    pub format: SampleFormat,
    pub channels: u8,
}

impl Default for PcmHwParams {
    fn default() -> Self {
        Self {
            sample_rate: 48_000,
            format: SampleFormat::S16Le,
            channels: 2,
        }
    }
}

impl PcmHwParams {
    /// Validate against the limited set of rates/formats/channel counts the
    /// PCM core and HDA backend in this kernel can actually drive.
    pub fn validate(&self) -> Result<(), &'static str> {
        match self.sample_rate {
            8_000 | 11_025 | 16_000 | 22_050 | 32_000 | 44_100 | 48_000 | 96_000 => {}
            _ => return Err("unsupported sample rate"),
        }
        if self.channels == 0 || self.channels > 8 {
            return Err("unsupported channel count");
        }
        Ok(())
    }

    /// Bytes per output frame (one sample per channel).
    pub fn frame_bytes(&self) -> usize {
        self.format.bytes() * self.channels as usize
    }
}

/// Minimal byte-oriented circular ring buffer with explicit backpressure:
/// `write()` returns the number of bytes actually accepted, which can be
/// less than the caller's slice length when the buffer is full. Callers
/// must retry/back off rather than assume all bytes landed.
#[derive(Debug, Clone)]
struct RingBuffer {
    data: Vec<u8>,
    capacity: usize,
    write_pos: usize,
    read_pos: usize,
    filled: usize,
    /// Total bytes ever rejected due to a full buffer (overrun counter).
    overruns: u64,
}

impl RingBuffer {
    fn new(capacity: usize) -> Self {
        let capacity = capacity.max(64);
        Self {
            data: vec![0u8; capacity],
            capacity,
            write_pos: 0,
            read_pos: 0,
            filled: 0,
            overruns: 0,
        }
    }

    fn write(&mut self, buf: &[u8]) -> usize {
        let space = self.capacity - self.filled;
        let n = core::cmp::min(buf.len(), space);
        let first = core::cmp::min(n, self.capacity - self.write_pos);
        self.data[self.write_pos..self.write_pos + first].copy_from_slice(&buf[..first]);
        if n > first {
            self.data[..n - first].copy_from_slice(&buf[first..n]);
        }
        self.write_pos = (self.write_pos + n) % self.capacity;
        self.filled += n;
        if n < buf.len() {
            self.overruns += (buf.len() - n) as u64;
        }
        n
    }

    fn read(&mut self, out: &mut [u8]) -> usize {
        let n = core::cmp::min(out.len(), self.filled);
        let first = core::cmp::min(n, self.capacity - self.read_pos);
        out[..first].copy_from_slice(&self.data[self.read_pos..self.read_pos + first]);
        if n > first {
            out[first..n].copy_from_slice(&self.data[..n - first]);
        }
        self.read_pos = (self.read_pos + n) % self.capacity;
        self.filled -= n;
        n
    }

    fn resize(&mut self, capacity: usize) {
        *self = RingBuffer::new(capacity);
    }
}

#[derive(Debug, Clone)]
pub struct PcmDevInfo {
    pub name: String,
    pub minor: u32,
    pub playback: bool,
}

/// Callback a hardware driver registers to receive PCM bytes written by
/// userspace directly, bypassing the software ring buffer. Returns the
/// number of bytes actually accepted (same short-write/backpressure
/// contract as `RingBuffer::write`).
pub type HwSink = Box<dyn Fn(&[u8]) -> usize + Send + Sync>;

struct PcmDevice {
    card: u32,
    device: u32,
    stream: PcmStream,
    name: String,
    ring: RingBuffer,
    hw_params: PcmHwParams,
    hw_sink: Option<HwSink>,
}

impl PcmDevice {
    fn new(card: u32, device: u32, stream: PcmStream) -> Self {
        let name = match stream {
            PcmStream::Playback => format!("pcmC{}D{}p", card, device),
            PcmStream::Capture => format!("pcmC{}D{}c", card, device),
        };
        Self {
            card,
            device,
            stream,
            name,
            ring: RingBuffer::new(DEFAULT_RING_BYTES),
            hw_params: PcmHwParams::default(),
            hw_sink: None,
        }
    }

    fn minor(&self) -> u32 {
        let stream_bit = match self.stream {
            PcmStream::Playback => 0,
            PcmStream::Capture => 1,
        };
        (self.card << 16) | (self.device << 8) | stream_bit
    }

    /// Capture path: pulls whatever bytes are queued in the ring buffer.
    /// There is no ADC/input DMA wired up in this kernel yet (see module
    /// docs / final report), so capture is purely a software loopback of
    /// previously buffered data; real silence is returned once drained.
    fn read(&mut self, buf: &mut [u8]) -> usize {
        if self.stream != PcmStream::Capture {
            return 0;
        }
        let n = self.ring.read(buf);
        if n < buf.len() {
            buf[n..].fill(0);
        }
        buf.len()
    }

    fn write(&mut self, buf: &[u8]) -> usize {
        if self.stream != PcmStream::Playback {
            return 0;
        }
        if let Some(sink) = &self.hw_sink {
            return sink(buf);
        }
        self.ring.write(buf)
    }
}

#[derive(Debug, Clone)]
struct SoundCard {
    #[allow(dead_code)]
    id: u32,
    name: String,
    pcm_ids: Vec<u32>,
}

static NEXT_CARD: AtomicU32 = AtomicU32::new(0);
static NEXT_PCM: AtomicU32 = AtomicU32::new(1);

static PCM_REGISTRY: RwLock<BTreeMap<u32, PcmDevice>> = RwLock::new(BTreeMap::new());
static CARD_REGISTRY: RwLock<BTreeMap<u32, SoundCard>> = RwLock::new(BTreeMap::new());
static PCM_BY_MINOR: RwLock<BTreeMap<u32, u32>> = RwLock::new(BTreeMap::new());

pub fn register_pcm(card: u32, device: u32, stream: PcmStream) -> u32 {
    let pcm = PcmDevice::new(card, device, stream);
    let minor = pcm.minor();
    let id = NEXT_PCM.fetch_add(1, Ordering::SeqCst);
    PCM_BY_MINOR.write().insert(minor, id);
    PCM_REGISTRY.write().insert(id, pcm);
    if let Some(c) = CARD_REGISTRY.write().get_mut(&card) {
        c.pcm_ids.push(id);
    }
    id
}

pub fn register_card(name: &str) -> u32 {
    let id = NEXT_CARD.fetch_add(1, Ordering::SeqCst);
    CARD_REGISTRY.write().insert(
        id,
        SoundCard {
            id,
            name: String::from(name),
            pcm_ids: Vec::new(),
        },
    );
    id
}

pub fn list_pcm_devices() -> Vec<PcmDevInfo> {
    PCM_REGISTRY
        .read()
        .values()
        .map(|pcm| PcmDevInfo {
            name: pcm.name.clone(),
            minor: pcm.minor(),
            playback: pcm.stream == PcmStream::Playback,
        })
        .collect()
}

pub fn pcm_read(minor: u32, buf: &mut [u8]) -> Option<usize> {
    let pcm_id = *PCM_BY_MINOR.read().get(&minor)?;
    PCM_REGISTRY
        .write()
        .get_mut(&pcm_id)
        .map(|pcm| pcm.read(buf))
}

pub fn pcm_write(minor: u32, buf: &[u8]) -> Option<usize> {
    let pcm_id = *PCM_BY_MINOR.read().get(&minor)?;
    PCM_REGISTRY
        .write()
        .get_mut(&pcm_id)
        .map(|pcm| pcm.write(buf))
}

/// Negotiate (and validate) hardware parameters for a PCM substream,
/// resizing its software ring buffer to hold roughly 200ms of audio at the
/// new rate. Mirrors a tiny slice of `snd_pcm_hw_params()`.
pub fn set_hw_params(minor: u32, params: PcmHwParams) -> Result<(), &'static str> {
    params.validate()?;
    let pcm_id = *PCM_BY_MINOR
        .read()
        .get(&minor)
        .ok_or("no such pcm device")?;
    let mut registry = PCM_REGISTRY.write();
    let pcm = registry.get_mut(&pcm_id).ok_or("no such pcm device")?;
    let bytes_per_sec = params.sample_rate as usize * params.frame_bytes();
    let ring_bytes = (bytes_per_sec / 5).max(4096); // ~200ms
    pcm.ring.resize(ring_bytes);
    pcm.hw_params = params;
    Ok(())
}

pub fn hw_params(minor: u32) -> Option<PcmHwParams> {
    let pcm_id = *PCM_BY_MINOR.read().get(&minor)?;
    PCM_REGISTRY.read().get(&pcm_id).map(|p| p.hw_params)
}

/// Bind a hardware sink (e.g. an HDA playback DMA buffer) to a playback PCM
/// device. Once bound, `pcm_write`/devfs writes go directly to hardware
/// instead of the software ring buffer. Returns an error if the minor does
/// not exist or is not a playback stream.
pub fn bind_hw_sink(minor: u32, sink: HwSink) -> Result<(), &'static str> {
    let pcm_id = *PCM_BY_MINOR
        .read()
        .get(&minor)
        .ok_or("no such pcm device")?;
    let mut registry = PCM_REGISTRY.write();
    let pcm = registry.get_mut(&pcm_id).ok_or("no such pcm device")?;
    if pcm.stream != PcmStream::Playback {
        return Err("hw sink only supported on playback streams");
    }
    pcm.hw_sink = Some(sink);
    Ok(())
}

/// Number of overrun (dropped due to backpressure) bytes recorded so far on
/// a PCM device's software ring buffer. Always 0 once a hardware sink is
/// bound, since the sink owns its own backpressure accounting.
pub fn overrun_bytes(minor: u32) -> Option<u64> {
    let pcm_id = *PCM_BY_MINOR.read().get(&minor)?;
    PCM_REGISTRY.read().get(&pcm_id).map(|p| p.ring.overruns)
}

pub fn init() -> Result<SoundInitStats, &'static str> {
    let card = register_card("RustOS");
    let playback_minor = {
        let id = register_pcm(card, 0, PcmStream::Playback);
        PCM_REGISTRY.read().get(&id).map(|p| p.minor())
    };
    register_pcm(card, 0, PcmStream::Capture);

    let nodes = crate::vfs::devfs::install_sound_nodes().map_err(|_| "snd devfs install")?;
    crate::serial_println!("sound: card {} registered, {} /dev/snd nodes", card, nodes);

    // Probe for a real Intel HDA controller and, if found, wire its
    // playback DMA buffer up as the hardware sink for our one playback
    // PCM device so that writes to /dev/snd/pcmC0D0p actually reach the
    // codec's DAC instead of just landing in the software ring buffer.
    match crate::drivers::sound::hda::probe_and_init() {
        Ok(Some(card_handle)) => {
            if let Some(minor) = playback_minor {
                let sink = card_handle.playback_sink();
                if bind_hw_sink(minor, sink).is_ok() {
                    crate::serial_println!(
                        "sound: HDA codec vendor/device {:#06x}:{:#06x} bound to pcm minor {:#x}",
                        card_handle.codec_vendor_id,
                        card_handle.codec_device_id,
                        minor
                    );
                }
            }
        }
        Ok(None) => {
            crate::serial_println!("sound: no Intel HDA controller present, software-only PCM");
        }
        Err(e) => {
            crate::serial_println!("sound: HDA probe failed: {}", e);
        }
    }

    Ok(SoundInitStats {
        cards: CARD_REGISTRY.read().len(),
        pcm_devices: PCM_REGISTRY.read().len(),
        dev_nodes: nodes,
    })
}

#[derive(Debug, Clone, Copy, Default)]
pub struct SoundInitStats {
    pub cards: usize,
    pub pcm_devices: usize,
    pub dev_nodes: usize,
}

pub fn pcm_count() -> usize {
    PCM_REGISTRY.read().len()
}
