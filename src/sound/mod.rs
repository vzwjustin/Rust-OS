//! ALSA-style PCM device registry and /dev/snd/* devfs integration

use alloc::collections::BTreeMap;
use alloc::format;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, Ordering};
use spin::RwLock;

pub const ALSA_MAJOR: u32 = 116;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PcmStream {
    Playback,
    Capture,
}

#[derive(Debug, Clone)]
pub struct PcmDevInfo {
    pub name: String,
    pub minor: u32,
    pub playback: bool,
}

#[derive(Debug, Clone)]
struct PcmDevice {
    card: u32,
    device: u32,
    stream: PcmStream,
    name: String,
    buffer: Vec<u8>,
    position: usize,
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
            buffer: vec![0u8; 4096],
            position: 0,
        }
    }

    fn minor(&self) -> u32 {
        let stream_bit = match self.stream {
            PcmStream::Playback => 0,
            PcmStream::Capture => 1,
        };
        (self.card << 16) | (self.device << 8) | stream_bit
    }

    fn read(&mut self, buf: &mut [u8]) -> usize {
        let len = core::cmp::min(buf.len(), self.buffer.len());
        if self.stream == PcmStream::Capture {
            buf[..len].copy_from_slice(&self.buffer[..len]);
        } else {
            buf[..len].fill(0);
        }
        self.position = self.position.wrapping_add(len);
        len
    }

    fn write(&mut self, buf: &[u8]) -> usize {
        if self.stream != PcmStream::Playback {
            return 0;
        }
        let len = core::cmp::min(buf.len(), self.buffer.len());
        self.buffer[..len].copy_from_slice(&buf[..len]);
        self.position = self.position.wrapping_add(len);
        len
    }
}

#[derive(Debug, Clone)]
struct SoundCard {
    id: u32,
    name: String,
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
    id
}

pub fn register_card(name: &str) -> u32 {
    let id = NEXT_CARD.fetch_add(1, Ordering::SeqCst);
    CARD_REGISTRY.write().insert(
        id,
        SoundCard {
            id,
            name: String::from(name),
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

pub fn init() -> Result<SoundInitStats, &'static str> {
    let card = register_card("RustOS");
    register_pcm(card, 0, PcmStream::Playback);
    register_pcm(card, 0, PcmStream::Capture);

    let nodes = crate::vfs::devfs::install_sound_nodes().map_err(|_| "snd devfs install")?;
    crate::serial_println!("sound: card {} registered, {} /dev/snd nodes", card, nodes);

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
