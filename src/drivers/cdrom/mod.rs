//! CD-ROM subsystem
//!
//! Provides CD-ROM device framework for optical disc drives.
//! Mirrors Linux's `drivers/cdrom/cdrom.c`.

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, Ordering};
use spin::RwLock;

// ── Types ───────────────────────────────────────────────────────────────

/// CD-ROM device (Linux `struct cdrom_device_info`).
pub struct CdromDevice {
    pub id: u32,
    pub name: String,
    pub ops: CdromOps,
    pub state: CdromState,
    pub media_present: bool,
    pub media_changed: bool,
    pub door_locked: bool,
    pub door_closed: bool,
    pub speed: u32,
    pub capacity: u64,
    pub disc_mode: DiscMode,
    pub toc: Vec<CdromTocEntry>,
    pub options: CdromOptions,
}

/// CD-ROM state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CdromState {
    Unregistered,
    Registered,
    Open,
    Closed,
    Playing,
    Paused,
}

/// Disc mode (Linux `enum cdrom_disc_mode`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiscMode {
    NoInfo,
    Audio,
    Data1,
    Data2,
    Xa21,
    Mixed,
    Unknown,
}

/// TOC entry (Linux `struct cdrom_tocentry`).
#[derive(Debug, Clone)]
pub struct CdromTocEntry {
    pub track: u8,
    pub lba: u32,
    pub adr_ctrl: u8,
    pub point: u8,
    pub min: u8,
    pub sec: u8,
    pub frame: u8,
    pub data: bool,
}

/// CD-ROM options (Linux `struct cdrom_device_info.options`).
#[derive(Debug, Clone)]
pub struct CdromOptions {
    pub auto_close: bool,
    pub auto_eject: bool,
    pub use_forced_eject: bool,
    pub lock_door: bool,
    pub debug: bool,
}

impl Default for CdromOptions {
    fn default() -> Self {
        Self {
            auto_close: true,
            auto_eject: false,
            use_forced_eject: true,
            lock_door: true,
            debug: false,
        }
    }
}

/// CD-ROM operations (Linux `struct cdrom_device_ops`).
pub struct CdromOps {
    pub open: fn(dev_id: u32) -> Result<(), &'static str>,
    pub release: fn(dev_id: u32) -> Result<(), &'static str>,
    pub drive_status: fn(dev_id: u32) -> Result<CdromDriveStatus, &'static str>,
    pub media_changed: fn(dev_id: u32) -> Result<bool, &'static str>,
    pub tray_move: fn(dev_id: u32, eject: bool) -> Result<(), &'static str>,
    pub lock_door: fn(dev_id: u32, lock: bool) -> Result<(), &'static str>,
    pub select_speed: fn(dev_id: u32, speed: u32) -> Result<(), &'static str>,
    pub get_last_session: fn(dev_id: u32) -> Result<(u32, u32), &'static str>,
    pub read_toc: fn(dev_id: u32) -> Result<Vec<CdromTocEntry>, &'static str>,
    pub read_lba:
        fn(dev_id: u32, lba: u32, count: u32, buf: &mut [u8]) -> Result<u32, &'static str>,
    pub play_audio: fn(dev_id: u32, lba_start: u32, lba_end: u32) -> Result<(), &'static str>,
    pub pause_audio: fn(dev_id: u32) -> Result<(), &'static str>,
    pub resume_audio: fn(dev_id: u32) -> Result<(), &'static str>,
    pub stop_audio: fn(dev_id: u32) -> Result<(), &'static str>,
    pub audio_status: fn(dev_id: u32) -> Result<CdromAudioStatus, &'static str>,
    pub reset: fn(dev_id: u32) -> Result<(), &'static str>,
}

/// CD-ROM drive status (Linux `enum cdrom_drive_status`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CdromDriveStatus {
    NoInfo,
    NoDisc,
    TrayOpen,
    DiscOk,
}

/// CD-ROM audio status (Linux `enum cdrom_audio_status`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CdromAudioStatus {
    NotValid,
    Playing,
    Paused,
    Completed,
    Error,
    NoStatus,
}

/// CD-ROM audio address (Linux `struct cdrom_msf`).
#[derive(Debug, Clone, Copy)]
pub struct CdromMsf {
    pub minute: u8,
    pub second: u8,
    pub frame: u8,
}

// ── Registry ────────────────────────────────────────────────────────────

static DEV_ID_COUNTER: AtomicU32 = AtomicU32::new(0);

static CDROM_DEVS: RwLock<BTreeMap<u32, CdromDevice>> = RwLock::new(BTreeMap::new());

// ── Public API ──────────────────────────────────────────────────────────

/// Register a CD-ROM device (Linux `register_cdrom`).
pub fn register_device(name: &str, speed: u32, ops: CdromOps) -> Result<u32, &'static str> {
    let id = DEV_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
    let dev = CdromDevice {
        id,
        name: String::from(name),
        ops,
        state: CdromState::Registered,
        media_present: false,
        media_changed: false,
        door_locked: false,
        door_closed: true,
        speed,
        capacity: 0,
        disc_mode: DiscMode::NoInfo,
        toc: Vec::new(),
        options: CdromOptions::default(),
    };
    CDROM_DEVS.write().insert(id, dev);
    Ok(id)
}

/// Open a CD-ROM device (Linux `cdrom_open`).
pub fn open(dev_id: u32) -> Result<(), &'static str> {
    let open_fn = {
        let devs = CDROM_DEVS.read();
        let dev = devs.get(&dev_id).ok_or("CD-ROM device not found")?;
        dev.ops.open
    };
    (open_fn)(dev_id)?;

    let mut devs = CDROM_DEVS.write();
    if let Some(dev) = devs.get_mut(&dev_id) {
        dev.state = CdromState::Open;
    }
    Ok(())
}

/// Release a CD-ROM device (Linux `cdrom_release`).
pub fn release(dev_id: u32) -> Result<(), &'static str> {
    let release_fn = {
        let devs = CDROM_DEVS.read();
        let dev = devs.get(&dev_id).ok_or("CD-ROM device not found")?;
        dev.ops.release
    };
    (release_fn)(dev_id)?;

    let mut devs = CDROM_DEVS.write();
    if let Some(dev) = devs.get_mut(&dev_id) {
        dev.state = CdromState::Closed;
        dev.media_changed = false;
    }
    Ok(())
}

/// Check drive status (Linux `cdrom_drive_status`).
pub fn drive_status(dev_id: u32) -> Result<CdromDriveStatus, &'static str> {
    let status_fn = {
        let devs = CDROM_DEVS.read();
        let dev = devs.get(&dev_id).ok_or("CD-ROM device not found")?;
        dev.ops.drive_status
    };
    let status = (status_fn)(dev_id)?;

    let mut devs = CDROM_DEVS.write();
    if let Some(dev) = devs.get_mut(&dev_id) {
        dev.media_present = status == CdromDriveStatus::DiscOk;
        dev.door_closed = status != CdromDriveStatus::TrayOpen;
    }
    Ok(status)
}

/// Check if media changed (Linux `cdrom_media_changed`).
pub fn media_changed(dev_id: u32) -> Result<bool, &'static str> {
    let changed_fn = {
        let devs = CDROM_DEVS.read();
        let dev = devs.get(&dev_id).ok_or("CD-ROM device not found")?;
        dev.ops.media_changed
    };
    (changed_fn)(dev_id)
}

/// Eject or close tray (Linux `cdrom_tray_move`).
pub fn tray_move(dev_id: u32, eject: bool) -> Result<(), &'static str> {
    let move_fn = {
        let devs = CDROM_DEVS.read();
        let dev = devs.get(&dev_id).ok_or("CD-ROM device not found")?;
        dev.ops.tray_move
    };
    (move_fn)(dev_id, eject)?;

    let mut devs = CDROM_DEVS.write();
    if let Some(dev) = devs.get_mut(&dev_id) {
        dev.door_closed = !eject;
        if eject {
            dev.media_present = false;
            dev.toc.clear();
        }
    }
    Ok(())
}

/// Lock/unlock the door (Linux `cdrom_lock_door`).
pub fn lock_door(dev_id: u32, lock: bool) -> Result<(), &'static str> {
    let lock_fn = {
        let devs = CDROM_DEVS.read();
        let dev = devs.get(&dev_id).ok_or("CD-ROM device not found")?;
        dev.ops.lock_door
    };
    (lock_fn)(dev_id, lock)?;

    let mut devs = CDROM_DEVS.write();
    if let Some(dev) = devs.get_mut(&dev_id) {
        dev.door_locked = lock;
    }
    Ok(())
}

/// Select drive speed (Linux `cdrom_select_speed`).
pub fn select_speed(dev_id: u32, speed: u32) -> Result<(), &'static str> {
    let speed_fn = {
        let devs = CDROM_DEVS.read();
        let dev = devs.get(&dev_id).ok_or("CD-ROM device not found")?;
        dev.ops.select_speed
    };
    (speed_fn)(dev_id, speed)?;

    let mut devs = CDROM_DEVS.write();
    if let Some(dev) = devs.get_mut(&dev_id) {
        dev.speed = speed;
    }
    Ok(())
}

/// Read TOC (Linux `cdrom_read_toc`).
pub fn read_toc(dev_id: u32) -> Result<Vec<CdromTocEntry>, &'static str> {
    let toc_fn = {
        let devs = CDROM_DEVS.read();
        let dev = devs.get(&dev_id).ok_or("CD-ROM device not found")?;
        if !dev.media_present {
            return Err("No media present");
        }
        dev.ops.read_toc
    };
    let toc = (toc_fn)(dev_id)?;

    let mut devs = CDROM_DEVS.write();
    if let Some(dev) = devs.get_mut(&dev_id) {
        dev.toc = toc.clone();
    }
    Ok(toc)
}

/// Read data from CD (Linux `cdrom_read_blocks`).
pub fn read_lba(dev_id: u32, lba: u32, count: u32, buf: &mut [u8]) -> Result<u32, &'static str> {
    let read_fn = {
        let devs = CDROM_DEVS.read();
        let dev = devs.get(&dev_id).ok_or("CD-ROM device not found")?;
        if !dev.media_present {
            return Err("No media present");
        }
        dev.ops.read_lba
    };
    (read_fn)(dev_id, lba, count, buf)
}

/// Play audio tracks (Linux `cdrom_play_audio`).
pub fn play_audio(dev_id: u32, lba_start: u32, lba_end: u32) -> Result<(), &'static str> {
    let play_fn = {
        let devs = CDROM_DEVS.read();
        let dev = devs.get(&dev_id).ok_or("CD-ROM device not found")?;
        dev.ops.play_audio
    };
    (play_fn)(dev_id, lba_start, lba_end)?;

    let mut devs = CDROM_DEVS.write();
    if let Some(dev) = devs.get_mut(&dev_id) {
        dev.state = CdromState::Playing;
    }
    Ok(())
}

/// Pause audio playback (Linux `cdrom_pause_audio`).
pub fn pause_audio(dev_id: u32) -> Result<(), &'static str> {
    let pause_fn = {
        let devs = CDROM_DEVS.read();
        let dev = devs.get(&dev_id).ok_or("CD-ROM device not found")?;
        dev.ops.pause_audio
    };
    (pause_fn)(dev_id)?;

    let mut devs = CDROM_DEVS.write();
    if let Some(dev) = devs.get_mut(&dev_id) {
        dev.state = CdromState::Paused;
    }
    Ok(())
}

/// Resume audio playback (Linux `cdrom_resume_audio`).
pub fn resume_audio(dev_id: u32) -> Result<(), &'static str> {
    let resume_fn = {
        let devs = CDROM_DEVS.read();
        let dev = devs.get(&dev_id).ok_or("CD-ROM device not found")?;
        dev.ops.resume_audio
    };
    (resume_fn)(dev_id)?;

    let mut devs = CDROM_DEVS.write();
    if let Some(dev) = devs.get_mut(&dev_id) {
        dev.state = CdromState::Playing;
    }
    Ok(())
}

/// Stop audio playback (Linux `cdrom_stop_audio`).
pub fn stop_audio(dev_id: u32) -> Result<(), &'static str> {
    let stop_fn = {
        let devs = CDROM_DEVS.read();
        let dev = devs.get(&dev_id).ok_or("CD-ROM device not found")?;
        dev.ops.stop_audio
    };
    (stop_fn)(dev_id)?;

    let mut devs = CDROM_DEVS.write();
    if let Some(dev) = devs.get_mut(&dev_id) {
        dev.state = CdromState::Open;
    }
    Ok(())
}

/// Reset the drive (Linux `cdrom_reset`).
pub fn reset(dev_id: u32) -> Result<(), &'static str> {
    let reset_fn = {
        let devs = CDROM_DEVS.read();
        let dev = devs.get(&dev_id).ok_or("CD-ROM device not found")?;
        dev.ops.reset
    };
    (reset_fn)(dev_id)
}

/// List all CD-ROM devices.
pub fn list_devices() -> Vec<(u32, String, u32, bool, bool)> {
    CDROM_DEVS
        .read()
        .iter()
        .map(|(id, d)| (*id, d.name.clone(), d.speed, d.media_present, d.door_closed))
        .collect()
}

/// Count registered devices.
pub fn device_count() -> usize {
    CDROM_DEVS.read().len()
}

// ── Software CD-ROM ─────────────────────────────────────────────────────

fn sw_open(_dev_id: u32) -> Result<(), &'static str> {
    Ok(())
}
fn sw_release(_dev_id: u32) -> Result<(), &'static str> {
    Ok(())
}
fn sw_drive_status(dev_id: u32) -> Result<CdromDriveStatus, &'static str> {
    let devs = CDROM_DEVS.read();
    let dev = devs.get(&dev_id).ok_or("CD-ROM device not found")?;
    if !dev.door_closed {
        Ok(CdromDriveStatus::TrayOpen)
    } else if dev.media_present {
        Ok(CdromDriveStatus::DiscOk)
    } else {
        Ok(CdromDriveStatus::NoDisc)
    }
}
fn sw_media_changed(dev_id: u32) -> Result<bool, &'static str> {
    let devs = CDROM_DEVS.read();
    let dev = devs.get(&dev_id).ok_or("CD-ROM device not found")?;
    Ok(dev.media_changed)
}
fn sw_tray_move(dev_id: u32, eject: bool) -> Result<(), &'static str> {
    if !eject {
        // Closing tray - simulate media present
        let mut devs = CDROM_DEVS.write();
        if let Some(dev) = devs.get_mut(&dev_id) {
            dev.media_present = true;
            dev.media_changed = true;
            dev.disc_mode = DiscMode::Data1;
            dev.capacity = 333_000; // ~650MB in 2048-byte sectors
        }
    }
    Ok(())
}
fn sw_lock_door(_dev_id: u32, _lock: bool) -> Result<(), &'static str> {
    Ok(())
}
fn sw_select_speed(_dev_id: u32, _speed: u32) -> Result<(), &'static str> {
    Ok(())
}
fn sw_get_last_session(_dev_id: u32) -> Result<(u32, u32), &'static str> {
    Ok((0, 16))
}
fn sw_read_toc(_dev_id: u32) -> Result<Vec<CdromTocEntry>, &'static str> {
    let mut toc = Vec::new();
    // Lead-in
    toc.push(CdromTocEntry {
        track: 0,
        lba: 0,
        adr_ctrl: 0x01,
        point: 0xA0,
        min: 0,
        sec: 0,
        frame: 0,
        data: false,
    });
    // Track 1 (data)
    toc.push(CdromTocEntry {
        track: 1,
        lba: 16,
        adr_ctrl: 0x14,
        point: 1,
        min: 0,
        sec: 2,
        frame: 0,
        data: true,
    });
    // Lead-out
    toc.push(CdromTocEntry {
        track: 0xAA,
        lba: 333_000,
        adr_ctrl: 0x14,
        point: 0xAA,
        min: 74,
        sec: 5,
        frame: 0,
        data: true,
    });
    Ok(toc)
}
fn sw_read_lba(_dev_id: u32, lba: u32, count: u32, buf: &mut [u8]) -> Result<u32, &'static str> {
    for (i, b) in buf.iter_mut().enumerate() {
        *b = ((lba as usize + i / 2048) & 0xFF) as u8;
    }
    Ok(count)
}
fn sw_play_audio(_dev_id: u32, _lba_start: u32, _lba_end: u32) -> Result<(), &'static str> {
    Ok(())
}
fn sw_pause_audio(_dev_id: u32) -> Result<(), &'static str> {
    Ok(())
}
fn sw_resume_audio(_dev_id: u32) -> Result<(), &'static str> {
    Ok(())
}
fn sw_stop_audio(_dev_id: u32) -> Result<(), &'static str> {
    Ok(())
}
fn sw_audio_status(dev_id: u32) -> Result<CdromAudioStatus, &'static str> {
    let devs = CDROM_DEVS.read();
    let dev = devs.get(&dev_id).ok_or("CD-ROM device not found")?;
    match dev.state {
        CdromState::Playing => Ok(CdromAudioStatus::Playing),
        CdromState::Paused => Ok(CdromAudioStatus::Paused),
        _ => Ok(CdromAudioStatus::NoStatus),
    }
}
fn sw_reset(_dev_id: u32) -> Result<(), &'static str> {
    Ok(())
}

/// Software CD-ROM ops.
pub fn software_cdrom_ops() -> CdromOps {
    CdromOps {
        open: sw_open,
        release: sw_release,
        drive_status: sw_drive_status,
        media_changed: sw_media_changed,
        tray_move: sw_tray_move,
        lock_door: sw_lock_door,
        select_speed: sw_select_speed,
        get_last_session: sw_get_last_session,
        read_toc: sw_read_toc,
        read_lba: sw_read_lba,
        play_audio: sw_play_audio,
        pause_audio: sw_pause_audio,
        resume_audio: sw_resume_audio,
        stop_audio: sw_stop_audio,
        audio_status: sw_audio_status,
        reset: sw_reset,
    }
}

// ── Init ────────────────────────────────────────────────────────────────

pub fn init() -> Result<(), &'static str> {
    crate::serial_println!("cdrom: subsystem ready");
    Ok(())
}
