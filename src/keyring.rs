//! Keyrings — kernel key management subsystem
//!
//! Ported from Linux security/keys/ (key.c, keyctl.c, process_keys.c).
//! Provides add_key, request_key, and keyctl syscalls with basic key/keyring management.

use alloc::collections::BTreeMap;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, Ordering};
use spin::{Mutex, RwLock};

// ── Key types ───────────────────────────────────────────────────────────

pub const KEY_TYPE_USER: &str = "user";
pub const KEY_TYPE_KEYRING: &str = "keyring";
pub const KEY_TYPE_LOGON: &str = "logon";

// ── Key permissions ─────────────────────────────────────────────────────

pub const KEY_POS_VIEW: u32 = 0x01000000;
pub const KEY_POS_READ: u32 = 0x02000000;
pub const KEY_POS_WRITE: u32 = 0x04000000;
pub const KEY_POS_SEARCH: u32 = 0x08000000;
pub const KEY_POS_LINK: u32 = 0x10000000;
pub const KEY_POS_SETATTR: u32 = 0x20000000;
pub const KEY_POS_ALL: u32 = 0x3f000000;

pub const KEY_USR_VIEW: u32 = 0x00010000;
pub const KEY_USR_READ: u32 = 0x00020000;
pub const KEY_USR_WRITE: u32 = 0x00040000;
pub const KEY_USR_SEARCH: u32 = 0x00080000;
pub const KEY_USR_LINK: u32 = 0x00100000;
pub const KEY_USR_SETATTR: u32 = 0x00200000;
pub const KEY_USR_ALL: u32 = 0x003f0000;

// ── keyctl commands ─────────────────────────────────────────────────────

pub const KEYCTL_GET_KEYRING_ID: u32 = 0;
pub const KEYCTL_JOIN_SESSION_KEYRING: u32 = 1;
pub const KEYCTL_UPDATE: u32 = 2;
pub const KEYCTL_REVOKE: u32 = 3;
pub const KEYCTL_CHOWN: u32 = 4;
pub const KEYCTL_SETPERM: u32 = 5;
pub const KEYCTL_DESCRIBE: u32 = 6;
pub const KEYCTL_CLEAR: u32 = 7;
pub const KEYCTL_LINK: u32 = 8;
pub const KEYCTL_UNLINK: u32 = 9;
pub const KEYCTL_SEARCH: u32 = 10;
pub const KEYCTL_READ: u32 = 11;
pub const KEYCTL_INSTANTIATE: u32 = 12;
pub const KEYCTL_NEGATE: u32 = 13;
pub const KEYCTL_SET_REQKEY_KEYRING: u32 = 14;
pub const KEYCTL_SET_TIMEOUT: u32 = 15;
pub const KEYCTL_ASSUME_AUTHORITY: u32 = 16;
pub const KEYCTL_GET_PERSISTENT: u32 = 22;
pub const KEYCTL_DH_COMPUTE: u32 = 23;
pub const KEYCTL_PKEY_QUERY: u32 = 24;
pub const KEYCTL_PKEY_ENCRYPT: u32 = 25;
pub const KEYCTL_PKEY_DECRYPT: u32 = 26;
pub const KEYCTL_PKEY_SIGN: u32 = 27;
pub const KEYCTL_PKEY_VERIFY: u32 = 28;
pub const KEYCTL_RESTRICT_KEYRING: u32 = 29;
pub const KEYCTL_MOVE: u32 = 30;
pub const KEYCTL_CAPABILITIES: u32 = 31;
pub const KEYCTL_WATCH_KEY: u32 = 32;

// ── Special keyring IDs ─────────────────────────────────────────────────

pub const KEY_SPEC_THREAD_KEYRING: i32 = -1;
pub const KEY_SPEC_PROCESS_KEYRING: i32 = -2;
pub const KEY_SPEC_SESSION_KEYRING: i32 = -3;
pub const KEY_SPEC_USER_KEYRING: i32 = -4;
pub const KEY_SPEC_USER_SESSION_KEYRING: i32 = -5;
pub const KEY_SPEC_GROUP_KEYRING: i32 = -6;
pub const KEY_SPEC_REQKEY_AUTH_KEY: i32 = -7;
pub const KEY_SPEC_REQUESTOR_KEYRING: i32 = -8;

// ── Key state ───────────────────────────────────────────────────────────

pub struct Key {
    pub id: u32,
    pub key_type: String,
    pub description: String,
    pub payload: Vec<u8>,
    pub perm: u32,
    pub uid: u32,
    pub gid: u32,
    pub revoked: bool,
    pub links: Vec<u32>, // Keys linked into this keyring
    pub is_keyring: bool,
}

impl Key {
    fn new(id: u32, key_type: &str, description: &str, payload: Vec<u8>, uid: u32) -> Self {
        Self {
            id,
            key_type: String::from(key_type),
            description: String::from(description),
            payload,
            perm: KEY_POS_ALL | KEY_USR_ALL,
            uid,
            gid: 0,
            revoked: false,
            links: Vec::new(),
            is_keyring: key_type == KEY_TYPE_KEYRING,
        }
    }
}

// ── Global state ────────────────────────────────────────────────────────

static KEYS: RwLock<BTreeMap<u32, Mutex<Key>>> = RwLock::new(BTreeMap::new());
static NEXT_KEY_ID: AtomicU32 = AtomicU32::new(1);
static SESSION_KEYRINGS: RwLock<BTreeMap<u32, u32>> = RwLock::new(BTreeMap::new());

// ── Syscall implementations ─────────────────────────────────────────────

/// add_key — add a key to a keyring
pub fn add_key(
    key_type: *const u8,
    description: *const u8,
    payload: *const u8,
    plen: usize,
    keyring_id: i32,
) -> i32 {
    if key_type.is_null() || description.is_null() {
        return -14;
    }

    let ktype = read_cstr(key_type);
    let desc = read_cstr(description);

    // Validate key type
    if ktype != KEY_TYPE_USER && ktype != KEY_TYPE_KEYRING && ktype != KEY_TYPE_LOGON {
        return -22; // EINVAL
    }

    // Read payload
    let payload_data = if payload.is_null() || plen == 0 {
        Vec::new()
    } else {
        unsafe { core::slice::from_raw_parts(payload, plen) }.to_vec()
    };

    let uid = crate::process::get_process_manager()
        .get_process(crate::process::current_pid())
        .map(|p| p.uid)
        .unwrap_or(0);

    let id = NEXT_KEY_ID.fetch_add(1, Ordering::SeqCst);
    let key = Key::new(id, &ktype, &desc, payload_data, uid);
    KEYS.write().insert(id, Mutex::new(key));

    // Link into specified keyring
    if keyring_id >= 0 {
        link_key_into_keyring(keyring_id as u32, id);
    } else {
        // Special keyring — for now, link into session keyring
        let pid = crate::process::current_pid();
        let sessions = SESSION_KEYRINGS.read();
        if let Some(&sk_id) = sessions.get(&pid) {
            drop(sessions);
            link_key_into_keyring(sk_id, id);
        }
    }

    crate::serial_println!("[keyring] add_key: type={} desc={} id={}", ktype, desc, id);
    id as i32
}

/// request_key — request a key by type and description
pub fn request_key(
    key_type: *const u8,
    description: *const u8,
    callout_info: *const u8,
    dest_keyring_id: i32,
) -> i32 {
    if key_type.is_null() || description.is_null() {
        return -14;
    }

    let ktype = read_cstr(key_type);
    let desc = read_cstr(description);

    // Search existing keys for a match
    let keys = KEYS.read();
    for (&id, key_mutex) in keys.iter() {
        let key = key_mutex.lock();
        if key.key_type == ktype && key.description == desc && !key.revoked {
            crate::serial_println!("[keyring] request_key: found existing id={}", id);
            return id as i32;
        }
    }
    drop(keys);

    // No existing key — would need to invoke callout (userspace helper)
    // For now, return ENOKEY
    let _ = callout_info;
    let _ = dest_keyring_id;
    -126 // ENOKEY
}

/// keyctl — various key control operations
pub fn keyctl(cmd: u32, arg2: u64, arg3: u64, arg4: u64, arg5: u64) -> i32 {
    match cmd {
        KEYCTL_GET_KEYRING_ID => {
            let id = arg2 as i32;
            let create = arg3 != 0;
            if id >= 0 {
                let keys = KEYS.read();
                if keys.contains_key(&(id as u32)) {
                    return id;
                }
                return -2; // ENOENT
            }
            // Special keyring
            if create {
                let pid = crate::process::current_pid();
                let mut sessions = SESSION_KEYRINGS.write();
                if let Some(&sk_id) = sessions.get(&pid) {
                    return sk_id as i32;
                }
                drop(sessions);
                // Create session keyring
                let new_id = NEXT_KEY_ID.fetch_add(1, Ordering::SeqCst);
                let key = Key::new(new_id, KEY_TYPE_KEYRING, "_ses", Vec::new(), 0);
                KEYS.write().insert(new_id, Mutex::new(key));
                SESSION_KEYRINGS.write().insert(pid, new_id);
                return new_id as i32;
            }
            -2
        }
        KEYCTL_JOIN_SESSION_KEYRING => {
            let pid = crate::process::current_pid();
            let name = if arg2 == 0 {
                // Join anonymous session keyring
                let new_id = NEXT_KEY_ID.fetch_add(1, Ordering::SeqCst);
                let key = Key::new(new_id, KEY_TYPE_KEYRING, "_ses", Vec::new(), 0);
                KEYS.write().insert(new_id, Mutex::new(key));
                SESSION_KEYRINGS.write().insert(pid, new_id);
                return new_id as i32;
            } else {
                read_cstr(arg2 as *const u8)
            };
            // Search for named keyring
            let keys = KEYS.read();
            for (&id, key_mutex) in keys.iter() {
                let key = key_mutex.lock();
                if key.is_keyring && key.description == name {
                    drop(key);
                    drop(keys);
                    SESSION_KEYRINGS.write().insert(pid, id);
                    return id as i32;
                }
            }
            -2
        }
        KEYCTL_UPDATE => {
            let id = arg2 as i32;
            let payload = arg3 as *const u8;
            let plen = arg4 as usize;
            if id < 0 {
                return -22;
            }
            let keys = KEYS.read();
            if let Some(key_mutex) = keys.get(&(id as u32)) {
                let mut key = key_mutex.lock();
                if key.revoked {
                    return -22;
                }
                if payload.is_null() || plen == 0 {
                    key.payload.clear();
                } else {
                    key.payload = unsafe { core::slice::from_raw_parts(payload, plen) }.to_vec();
                }
                return 0;
            }
            -2
        }
        KEYCTL_REVOKE => {
            let id = arg2 as i32;
            if id < 0 {
                return -22;
            }
            let keys = KEYS.read();
            if let Some(key_mutex) = keys.get(&(id as u32)) {
                key_mutex.lock().revoked = true;
                return 0;
            }
            -2
        }
        KEYCTL_READ => {
            let id = arg2 as i32;
            let buf = arg3 as *mut u8;
            let buflen = arg4 as usize;
            if id < 0 {
                return -22;
            }
            let keys = KEYS.read();
            if let Some(key_mutex) = keys.get(&(id as u32)) {
                let key = key_mutex.lock();
                if key.is_keyring {
                    // Return list of key IDs in the keyring
                    let total = key.links.len() * 4;
                    if buf.is_null() || buflen == 0 {
                        return total as i32;
                    }
                    let count = core::cmp::min(key.links.len(), buflen / 4);
                    for i in 0..count {
                        unsafe {
                            core::ptr::write(buf.add(i * 4) as *mut u32, key.links[i]);
                        }
                    }
                    return (count * 4) as i32;
                }
                let total = key.payload.len();
                if buf.is_null() || buflen == 0 {
                    return total as i32;
                }
                let copy_len = core::cmp::min(total, buflen);
                unsafe {
                    core::ptr::copy_nonoverlapping(key.payload.as_ptr(), buf, copy_len);
                }
                return copy_len as i32;
            }
            -2
        }
        KEYCTL_CLEAR => {
            let id = arg2 as i32;
            if id < 0 {
                return -22;
            }
            let keys = KEYS.read();
            if let Some(key_mutex) = keys.get(&(id as u32)) {
                let mut key = key_mutex.lock();
                if !key.is_keyring {
                    return -22;
                }
                key.links.clear();
                return 0;
            }
            -2
        }
        KEYCTL_LINK => {
            let key_id = arg2 as i32;
            let keyring_id = arg3 as i32;
            if key_id < 0 || keyring_id < 0 {
                return -22;
            }
            link_key_into_keyring(keyring_id as u32, key_id as u32)
        }
        KEYCTL_UNLINK => {
            let key_id = arg2 as i32;
            let keyring_id = arg3 as i32;
            if key_id < 0 || keyring_id < 0 {
                return -22;
            }
            let keys = KEYS.read();
            if let Some(kr_mutex) = keys.get(&(keyring_id as u32)) {
                let mut kr = kr_mutex.lock();
                if let Some(pos) = kr.links.iter().position(|&k| k == key_id as u32) {
                    kr.links.remove(pos);
                    return 0;
                }
                return -2;
            }
            -2
        }
        KEYCTL_SEARCH => {
            let keyring_id = arg2 as i32;
            let ktype = read_cstr(arg3 as *const u8);
            let desc = read_cstr(arg4 as *const u8);
            let _dest = arg5 as i32;

            let keys = KEYS.read();
            for (&id, key_mutex) in keys.iter() {
                let key = key_mutex.lock();
                if key.key_type == ktype && key.description == desc && !key.revoked {
                    return id as i32;
                }
            }
            -126 // ENOKEY
        }
        KEYCTL_DESCRIBE => {
            let id = arg2 as i32;
            let buf = arg3 as *mut u8;
            let buflen = arg4 as usize;
            if id < 0 {
                return -22;
            }
            let keys = KEYS.read();
            if let Some(key_mutex) = keys.get(&(id as u32)) {
                let key = key_mutex.lock();
                // Format: "type;uid;gid;perm;description"
                let desc_str = key.key_type.to_string()
                    + ";"
                    + &key.uid.to_string()
                    + ";"
                    + &key.gid.to_string()
                    + ";"
                    + &key.perm.to_string()
                    + ";"
                    + &key.description;
                let bytes = desc_str.as_bytes();
                let total = bytes.len() + 1;
                if buf.is_null() || buflen == 0 {
                    return total as i32;
                }
                let copy_len = core::cmp::min(total, buflen);
                unsafe {
                    core::ptr::copy_nonoverlapping(bytes.as_ptr(), buf, copy_len.saturating_sub(1));
                    if copy_len > 0 {
                        *buf.add(copy_len - 1) = 0;
                    }
                }
                return copy_len as i32;
            }
            -2
        }
        KEYCTL_SETPERM => {
            let id = arg2 as i32;
            let perm = arg3 as u32;
            if id < 0 {
                return -22;
            }
            let keys = KEYS.read();
            if let Some(key_mutex) = keys.get(&(id as u32)) {
                key_mutex.lock().perm = perm;
                return 0;
            }
            -2
        }
        KEYCTL_CHOWN => {
            let id = arg2 as i32;
            let uid = arg3 as u32;
            let gid = arg4 as u32;
            if id < 0 {
                return -22;
            }
            let keys = KEYS.read();
            if let Some(key_mutex) = keys.get(&(id as u32)) {
                let mut key = key_mutex.lock();
                key.uid = uid;
                key.gid = gid;
                return 0;
            }
            -2
        }
        KEYCTL_SET_TIMEOUT => {
            let id = arg2 as i32;
            let _timeout = arg3 as u32;
            if id < 0 {
                return -22;
            }
            let keys = KEYS.read();
            if keys.contains_key(&(id as u32)) {
                // We don't implement timeouts yet — accept silently
                return 0;
            }
            -2
        }
        KEYCTL_CAPABILITIES => {
            let buf = arg2 as *mut u8;
            let buflen = arg3 as usize;
            // Return capabilities bitmask
            let caps: [u8; 8] = [0x03, 0, 0, 0, 0, 0, 0, 0]; // Basic capabilities
            if !buf.is_null() && buflen >= 8 {
                unsafe {
                    core::ptr::copy_nonoverlapping(caps.as_ptr(), buf, 8);
                }
            }
            8
        }
        _ => {
            // Unsupported keyctl commands
            -38 // ENOSYS
        }
    }
}

// ── Helpers ─────────────────────────────────────────────────────────────

fn read_cstr(ptr: *const u8) -> String {
    if ptr.is_null() {
        return String::new();
    }
    let mut len = 0;
    while unsafe { *ptr.add(len) } != 0 {
        len += 1;
    }
    let bytes = unsafe { core::slice::from_raw_parts(ptr, len) };
    String::from_utf8_lossy(bytes).into_owned()
}

fn link_key_into_keyring(keyring_id: u32, key_id: u32) -> i32 {
    let keys = KEYS.read();
    if let Some(kr_mutex) = keys.get(&keyring_id) {
        let mut kr = kr_mutex.lock();
        if !kr.is_keyring {
            return -22;
        }
        if !kr.links.contains(&key_id) {
            kr.links.push(key_id);
        }
        return 0;
    }
    -2
}

/// Close a key (called when fd is closed).
pub fn close_key(id: u32) {
    KEYS.write().remove(&id);
}

/// Initialize the keyring subsystem.
pub fn init() {
    // Create initial session keyring for PID 1
    let id = NEXT_KEY_ID.fetch_add(1, Ordering::SeqCst);
    let key = Key::new(id, KEY_TYPE_KEYRING, "_ses", Vec::new(), 0);
    KEYS.write().insert(id, Mutex::new(key));
    SESSION_KEYRINGS.write().insert(1, id);
    crate::serial_println!("[keyring] Keyring subsystem initialized");
}
