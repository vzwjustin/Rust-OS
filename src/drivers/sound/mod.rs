//! Sound/ALSA subsystem
//!
//! Provides ALSA-compatible audio framework with sound cards, PCM devices,
//! controls, and mixer support. Mirrors Linux's `sound/core/*`.

pub mod hda;

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, Ordering};
use spin::RwLock;

// ── Types ───────────────────────────────────────────────────────────────

/// PCM stream direction (Linux `enum snd_pcm_stream`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PcmStream {
    Playback,
    Capture,
}

/// PCM format (Linux `enum snd_pcm_format` subset).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PcmFormat {
    S8,
    U8,
    S16Le,
    S16Be,
    U16Le,
    U16Be,
    S24Le,
    S24Be,
    S32Le,
    S32Be,
    F32Le,
    F32Be,
}

impl PcmFormat {
    pub fn physical_width(&self) -> u32 {
        match self {
            PcmFormat::S8 | PcmFormat::U8 => 8,
            PcmFormat::S16Le | PcmFormat::S16Be | PcmFormat::U16Le | PcmFormat::U16Be => 16,
            PcmFormat::S24Le | PcmFormat::S24Be => 24,
            PcmFormat::S32Le | PcmFormat::S32Be | PcmFormat::F32Le | PcmFormat::F32Be => 32,
        }
    }
}

/// PCM subformat (Linux `enum snd_pcm_subformat`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PcmSubformat {
    Standard,
    Other,
}

/// PCM state (Linux `enum snd_pcm_state`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PcmState {
    Open,
    Setup,
    Prepared,
    Running,
    Xrun,
    Draining,
    Paused,
    Suspended,
    Disconnect,
}

/// PCM hardware parameters (Linux `struct snd_pcm_hardware`).
#[derive(Debug, Clone)]
pub struct PcmHwParams {
    pub format: PcmFormat,
    pub channels: u32,
    pub rate: u32,
    pub buffer_size: u32,
    pub period_size: u32,
    pub periods: u32,
}

/// PCM device (Linux `struct snd_pcm`).
pub struct PcmDevice {
    pub id: u32,
    pub name: String,
    pub stream: PcmStream,
    pub state: PcmState,
    pub hw_params: Option<PcmHwParams>,
    pub buffer_bytes: u64,
    pub appl_ptr: u64,
    pub hw_ptr: u64,
    pub ops: PcmOps,
}

/// PCM operations (Linux `struct snd_pcm_ops`).
pub struct PcmOps {
    pub open: fn(pcm_id: u32) -> Result<(), &'static str>,
    pub close: fn(pcm_id: u32) -> Result<(), &'static str>,
    pub hw_params: fn(pcm_id: u32, params: &PcmHwParams) -> Result<(), &'static str>,
    pub prepare: fn(pcm_id: u32) -> Result<(), &'static str>,
    pub trigger: fn(pcm_id: u32, cmd: PcmTriggerCmd) -> Result<(), &'static str>,
    pub pointer: fn(pcm_id: u32) -> Result<u64, &'static str>,
    pub copy: fn(pcm_id: u32, buf: &mut [u8], offset: u64) -> Result<usize, &'static str>,
    pub silence: fn(pcm_id: u32, offset: u64, frames: u32) -> Result<(), &'static str>,
}

/// PCM trigger command (Linux `enum snd_pcm_trigger`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PcmTriggerCmd {
    Start,
    Stop,
    PausePush,
    PauseRelease,
    Drain,
    Suspend,
    Resume,
}

/// Mixer control element (Linux `struct snd_kcontrol`).
pub struct MixerControl {
    pub id: u32,
    pub name: String,
    pub control_type: MixerControlType,
    pub min: i32,
    pub max: i32,
    pub value: i32,
    pub channel_count: u32,
}

/// Mixer control type (Linux `enum snd_ctl_elem_type`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MixerControlType {
    Boolean,
    Integer,
    Enumerated,
    Bytes,
    Iec958,
    Integer64,
}

/// Sound card (Linux `struct snd_card`).
pub struct SoundCard {
    pub id: u32,
    pub name: String,
    pub pcm_ids: Vec<u32>,
    pub mixer_controls: Vec<u32>,
}

// ── Registry ────────────────────────────────────────────────────────────

static CARD_ID_COUNTER: AtomicU32 = AtomicU32::new(0);
static PCM_ID_COUNTER: AtomicU32 = AtomicU32::new(0);
static MIXER_ID_COUNTER: AtomicU32 = AtomicU32::new(0);

static SOUND_CARDS: RwLock<BTreeMap<u32, SoundCard>> = RwLock::new(BTreeMap::new());
static PCM_DEVICES: RwLock<BTreeMap<u32, PcmDevice>> = RwLock::new(BTreeMap::new());
static MIXER_CONTROLS: RwLock<BTreeMap<u32, MixerControl>> = RwLock::new(BTreeMap::new());

// ── Public API ──────────────────────────────────────────────────────────

/// Register a sound card.
pub fn register_card(name: &str) -> Result<u32, &'static str> {
    let id = CARD_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
    let card = SoundCard {
        id,
        name: String::from(name),
        pcm_ids: Vec::new(),
        mixer_controls: Vec::new(),
    };
    SOUND_CARDS.write().insert(id, card);
    Ok(id)
}

/// Register a PCM device on a sound card.
pub fn register_pcm(
    card_id: u32,
    name: &str,
    stream: PcmStream,
    ops: PcmOps,
) -> Result<u32, &'static str> {
    let pcm_id = PCM_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
    let pcm = PcmDevice {
        id: pcm_id,
        name: String::from(name),
        stream,
        state: PcmState::Open,
        hw_params: None,
        buffer_bytes: 0,
        appl_ptr: 0,
        hw_ptr: 0,
        ops,
    };
    PCM_DEVICES.write().insert(pcm_id, pcm);

    let mut cards = SOUND_CARDS.write();
    let card = cards.get_mut(&card_id).ok_or("Sound card not found")?;
    card.pcm_ids.push(pcm_id);
    Ok(pcm_id)
}

/// Register a mixer control on a sound card.
pub fn register_mixer_control(
    card_id: u32,
    name: &str,
    control_type: MixerControlType,
    min: i32,
    max: i32,
    value: i32,
    channel_count: u32,
) -> Result<u32, &'static str> {
    let ctrl_id = MIXER_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
    let ctrl = MixerControl {
        id: ctrl_id,
        name: String::from(name),
        control_type,
        min,
        max,
        value,
        channel_count,
    };
    MIXER_CONTROLS.write().insert(ctrl_id, ctrl);

    let mut cards = SOUND_CARDS.write();
    let card = cards.get_mut(&card_id).ok_or("Sound card not found")?;
    card.mixer_controls.push(ctrl_id);
    Ok(ctrl_id)
}

/// Set PCM hardware parameters.
pub fn pcm_hw_params(pcm_id: u32, params: PcmHwParams) -> Result<(), &'static str> {
    let hw_params_fn = {
        let pcms = PCM_DEVICES.read();
        let pcm = pcms.get(&pcm_id).ok_or("PCM device not found")?;
        pcm.ops.hw_params
    };
    (hw_params_fn)(pcm_id, &params)?;

    let mut pcms = PCM_DEVICES.write();
    let pcm = pcms.get_mut(&pcm_id).ok_or("PCM device not found")?;
    let frame_size = (params.channels * params.format.physical_width() / 8) as u64;
    pcm.buffer_bytes = frame_size * params.buffer_size as u64;
    pcm.hw_params = Some(params);
    pcm.state = PcmState::Setup;
    Ok(())
}

/// Prepare a PCM for playback/capture.
pub fn pcm_prepare(pcm_id: u32) -> Result<(), &'static str> {
    let prepare_fn = {
        let pcms = PCM_DEVICES.read();
        let pcm = pcms.get(&pcm_id).ok_or("PCM device not found")?;
        pcm.ops.prepare
    };
    (prepare_fn)(pcm_id)?;

    let mut pcms = PCM_DEVICES.write();
    let pcm = pcms.get_mut(&pcm_id).ok_or("PCM device not found")?;
    pcm.state = PcmState::Prepared;
    pcm.appl_ptr = 0;
    pcm.hw_ptr = 0;
    Ok(())
}

/// Trigger PCM start/stop/pause.
pub fn pcm_trigger(pcm_id: u32, cmd: PcmTriggerCmd) -> Result<(), &'static str> {
    let trigger_fn = {
        let pcms = PCM_DEVICES.read();
        let pcm = pcms.get(&pcm_id).ok_or("PCM device not found")?;
        pcm.ops.trigger
    };
    (trigger_fn)(pcm_id, cmd)?;

    let mut pcms = PCM_DEVICES.write();
    let pcm = pcms.get_mut(&pcm_id).ok_or("PCM device not found")?;
    pcm.state = match cmd {
        PcmTriggerCmd::Start | PcmTriggerCmd::Resume => PcmState::Running,
        PcmTriggerCmd::Stop => PcmState::Setup,
        PcmTriggerCmd::PausePush => PcmState::Paused,
        PcmTriggerCmd::PauseRelease => PcmState::Running,
        PcmTriggerCmd::Suspend => PcmState::Suspended,
        PcmTriggerCmd::Drain => PcmState::Draining,
    };
    Ok(())
}

/// Get the current hardware pointer (in frames).
pub fn pcm_pointer(pcm_id: u32) -> Result<u64, &'static str> {
    let pointer_fn = {
        let pcms = PCM_DEVICES.read();
        let pcm = pcms.get(&pcm_id).ok_or("PCM device not found")?;
        pcm.ops.pointer
    };
    let ptr = (pointer_fn)(pcm_id)?;

    let mut pcms = PCM_DEVICES.write();
    if let Some(pcm) = pcms.get_mut(&pcm_id) {
        pcm.hw_ptr = ptr;
    }
    Ok(ptr)
}

/// Write audio data to a playback PCM (or read from capture PCM).
pub fn pcm_transfer(pcm_id: u32, buf: &mut [u8], offset: u64) -> Result<usize, &'static str> {
    let copy_fn = {
        let pcms = PCM_DEVICES.read();
        let pcm = pcms.get(&pcm_id).ok_or("PCM device not found")?;
        if pcm.state != PcmState::Running && pcm.state != PcmState::Prepared {
            return Err("PCM not running or prepared");
        }
        pcm.ops.copy
    };
    let n = (copy_fn)(pcm_id, buf, offset)?;

    let mut pcms = PCM_DEVICES.write();
    if let Some(pcm) = pcms.get_mut(&pcm_id) {
        pcm.appl_ptr += n as u64;
    }
    Ok(n)
}

/// Get PCM state.
pub fn pcm_get_state(pcm_id: u32) -> Result<PcmState, &'static str> {
    let pcms = PCM_DEVICES.read();
    let pcm = pcms.get(&pcm_id).ok_or("PCM device not found")?;
    Ok(pcm.state)
}

/// Set mixer control value.
pub fn mixer_set_value(ctrl_id: u32, value: i32) -> Result<(), &'static str> {
    let mut ctrls = MIXER_CONTROLS.write();
    let ctrl = ctrls.get_mut(&ctrl_id).ok_or("Mixer control not found")?;
    if value < ctrl.min || value > ctrl.max {
        return Err("Mixer value out of range");
    }
    ctrl.value = value;
    Ok(())
}

/// Get mixer control value.
pub fn mixer_get_value(ctrl_id: u32) -> Result<i32, &'static str> {
    let ctrls = MIXER_CONTROLS.read();
    let ctrl = ctrls.get(&ctrl_id).ok_or("Mixer control not found")?;
    Ok(ctrl.value)
}

/// List sound cards.
pub fn list_cards() -> Vec<(u32, String)> {
    SOUND_CARDS
        .read()
        .iter()
        .map(|(id, c)| (*id, c.name.clone()))
        .collect()
}

/// List PCM devices on a card.
pub fn list_pcms(card_id: u32) -> Result<Vec<(u32, String, PcmStream)>, &'static str> {
    let cards = SOUND_CARDS.read();
    let card = cards.get(&card_id).ok_or("Sound card not found")?;
    let pcms = PCM_DEVICES.read();
    let mut result = Vec::new();
    for &pcm_id in &card.pcm_ids {
        if let Some(pcm) = pcms.get(&pcm_id) {
            result.push((pcm_id, pcm.name.clone(), pcm.stream));
        }
    }
    Ok(result)
}

/// Count registered cards.
pub fn card_count() -> usize {
    SOUND_CARDS.read().len()
}

// ── Software PCM (null audio sink) ──────────────────────────────────────

fn sw_pcm_open(_pcm_id: u32) -> Result<(), &'static str> {
    Ok(())
}
fn sw_pcm_close(_pcm_id: u32) -> Result<(), &'static str> {
    Ok(())
}
fn sw_pcm_hw_params(_pcm_id: u32, _params: &PcmHwParams) -> Result<(), &'static str> {
    Ok(())
}
fn sw_pcm_prepare(_pcm_id: u32) -> Result<(), &'static str> {
    Ok(())
}
fn sw_pcm_trigger(_pcm_id: u32, _cmd: PcmTriggerCmd) -> Result<(), &'static str> {
    Ok(())
}
fn sw_pcm_pointer(pcm_id: u32) -> Result<u64, &'static str> {
    let pcms = PCM_DEVICES.read();
    let pcm = pcms.get(&pcm_id).ok_or("PCM not found")?;
    Ok(pcm.appl_ptr)
}
fn sw_pcm_copy(_pcm_id: u32, buf: &mut [u8], _offset: u64) -> Result<usize, &'static str> {
    for b in buf.iter_mut() {
        *b = 0;
    }
    Ok(buf.len())
}
fn sw_pcm_silence(_pcm_id: u32, _offset: u64, _frames: u32) -> Result<(), &'static str> {
    Ok(())
}

/// Software PCM ops (null audio — discards playback, returns silence for capture).
pub fn software_pcm_ops() -> PcmOps {
    PcmOps {
        open: sw_pcm_open,
        close: sw_pcm_close,
        hw_params: sw_pcm_hw_params,
        prepare: sw_pcm_prepare,
        trigger: sw_pcm_trigger,
        pointer: sw_pcm_pointer,
        copy: sw_pcm_copy,
        silence: sw_pcm_silence,
    }
}

// ── Init ────────────────────────────────────────────────────────────────

pub fn init() -> Result<(), &'static str> {
    let card_id = register_card("sw-audio")?;

    let ops = software_pcm_ops();
    register_pcm(card_id, "sw-playback", PcmStream::Playback, ops)?;

    let ops2 = software_pcm_ops();
    register_pcm(card_id, "sw-capture", PcmStream::Capture, ops2)?;

    register_mixer_control(
        card_id,
        "Master Playback Volume",
        MixerControlType::Integer,
        0,
        100,
        75,
        2,
    )?;
    register_mixer_control(
        card_id,
        "Master Capture Volume",
        MixerControlType::Integer,
        0,
        100,
        50,
        2,
    )?;
    register_mixer_control(
        card_id,
        "Master Playback Switch",
        MixerControlType::Boolean,
        0,
        1,
        1,
        2,
    )?;

    Ok(())
}
