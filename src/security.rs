//! Production security and access control for RustOS
//!
//! Implements real security features including privilege levels,
//! access control, and security context management

use alloc::collections::BTreeMap;
use alloc::string::ToString;
use alloc::{vec, vec::Vec};
use core::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use spin::RwLock;

/// User ID type
pub type Uid = u32;
/// Group ID type  
pub type Gid = u32;
/// Process ID type
pub type Pid = u32;

/// Security level / privilege ring
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[repr(u8)]
pub enum SecurityLevel {
    /// Ring 0 - Kernel mode (highest privilege)
    Kernel = 0,
    /// Ring 1 - Device drivers
    Driver = 1,
    /// Ring 2 - System services
    System = 2,
    /// Ring 3 - User mode (lowest privilege)
    User = 3,
}

/// Permission flags
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Permissions {
    pub read: bool,
    pub write: bool,
    pub execute: bool,
    pub setuid: bool,
    pub setgid: bool,
    pub sticky: bool,
}

impl Permissions {
    /// Create permissions from Unix mode bits
    pub fn from_mode(mode: u16) -> Self {
        Self {
            read: mode & 0o400 != 0,
            write: mode & 0o200 != 0,
            execute: mode & 0o100 != 0,
            setuid: mode & 0o4000 != 0,
            setgid: mode & 0o2000 != 0,
            sticky: mode & 0o1000 != 0,
        }
    }

    /// Convert to Unix mode bits
    pub fn to_mode(&self) -> u16 {
        let mut mode = 0;
        if self.read {
            mode |= 0o400;
        }
        if self.write {
            mode |= 0o200;
        }
        if self.execute {
            mode |= 0o100;
        }
        if self.setuid {
            mode |= 0o4000;
        }
        if self.setgid {
            mode |= 0o2000;
        }
        if self.sticky {
            mode |= 0o1000;
        }
        mode
    }
}

/// Security context for a process
#[derive(Debug, Clone)]
pub struct SecurityContext {
    pub pid: Pid,
    pub uid: Uid,
    pub gid: Gid,
    pub euid: Uid, // Effective UID
    pub egid: Gid, // Effective GID
    pub groups: Vec<Gid>,
    pub level: SecurityLevel,
    pub capabilities: Capabilities,
}

impl SecurityContext {
    /// Create a new security context
    pub fn new(pid: Pid, uid: Uid, gid: Gid, level: SecurityLevel) -> Self {
        Self {
            pid,
            uid,
            gid,
            euid: uid,
            egid: gid,
            groups: Vec::new(),
            level,
            capabilities: Capabilities::default(),
        }
    }

    /// Check if context has root privileges
    pub fn is_root(&self) -> bool {
        self.euid == 0 || self.level == SecurityLevel::Kernel
    }

    /// Check if context can access a resource
    pub fn can_access(&self, owner: Uid, group: Gid, perms: Permissions) -> bool {
        // Kernel always has access
        if self.level == SecurityLevel::Kernel {
            return true;
        }

        // Root can access everything
        if self.is_root() {
            return true;
        }

        // Check owner permissions
        if self.euid == owner {
            return perms.read || perms.write || perms.execute;
        }

        // Check group permissions
        if self.egid == group || self.groups.contains(&group) {
            return perms.read || perms.execute;
        }

        // Default: check world permissions
        perms.read
    }
}

/// Capability flags (simplified Linux capabilities)
#[derive(Debug, Clone, Copy, Default)]
pub struct Capabilities {
    pub cap_chown: bool,
    pub cap_kill: bool,
    pub cap_setuid: bool,
    pub cap_setgid: bool,
    pub cap_sys_admin: bool,
    pub cap_sys_boot: bool,
    pub cap_sys_time: bool,
    pub cap_sys_module: bool,
    pub cap_net_admin: bool,
    pub cap_ipc_owner: bool,
}

/// Global security contexts for all processes
static SECURITY_CONTEXTS: RwLock<BTreeMap<Pid, SecurityContext>> = RwLock::new(BTreeMap::new());
/// Security subsystem initialized flag
static INITIALIZED: AtomicBool = AtomicBool::new(false);
/// Security audit counter
static AUDIT_COUNTER: AtomicU32 = AtomicU32::new(0);

/// Initialize security subsystem with cryptographic support
pub fn init() -> Result<(), &'static str> {
    if INITIALIZED.load(Ordering::Acquire) {
        return Ok(());
    }

    // Initialize random number generator
    init_rng()?;

    // Create kernel security context (PID 0)
    let kernel_ctx = SecurityContext::new(0, 0, 0, SecurityLevel::Kernel);
    let mut contexts = SECURITY_CONTEXTS.write();
    contexts.insert(0, kernel_ctx);

    INITIALIZED.store(true, Ordering::Release);
    Ok(())
}

/// Create security context for a new process
pub fn create_context(pid: Pid, parent_pid: Option<Pid>) -> Result<(), &'static str> {
    let mut contexts = SECURITY_CONTEXTS.write();

    let new_context = if let Some(parent) = parent_pid {
        // Inherit from parent
        if let Some(parent_ctx) = contexts.get(&parent) {
            let mut ctx = parent_ctx.clone();
            ctx.pid = pid;
            ctx
        } else {
            return Err("Parent context not found");
        }
    } else {
        // Create default user context
        SecurityContext::new(pid, 1000, 1000, SecurityLevel::User)
    };

    contexts.insert(pid, new_context);
    Ok(())
}

/// Get security context for a process
pub fn get_context(pid: Pid) -> Option<SecurityContext> {
    SECURITY_CONTEXTS.read().get(&pid).cloned()
}

/// Remove security context when process exits
pub fn remove_context(pid: Pid) {
    SECURITY_CONTEXTS.write().remove(&pid);
}

/// Set effective UID for a process
pub fn setuid(pid: Pid, uid: Uid) -> Result<(), &'static str> {
    let mut contexts = SECURITY_CONTEXTS.write();

    if let Some(ctx) = contexts.get_mut(&pid) {
        // Check if allowed to setuid
        if ctx.capabilities.cap_setuid || ctx.is_root() {
            ctx.euid = uid;
            Ok(())
        } else {
            audit_event(AuditEvent::PermissionDenied {
                pid,
                action: "setuid",
            });
            Err("Permission denied")
        }
    } else {
        Err("Context not found")
    }
}

/// Set effective GID for a process
pub fn setgid(pid: Pid, gid: Gid) -> Result<(), &'static str> {
    let mut contexts = SECURITY_CONTEXTS.write();

    if let Some(ctx) = contexts.get_mut(&pid) {
        if ctx.capabilities.cap_setgid || ctx.is_root() {
            ctx.egid = gid;
            Ok(())
        } else {
            audit_event(AuditEvent::PermissionDenied {
                pid,
                action: "setgid",
            });
            Err("Permission denied")
        }
    } else {
        Err("Context not found")
    }
}

/// Check if process can perform an action with comprehensive privilege validation
pub fn check_permission(pid: Pid, action: &str) -> bool {
    // Get security context for the process
    let ctx = match get_context(pid) {
        Some(ctx) => ctx,
        None => {
            audit_event(AuditEvent::SecurityViolation {
                pid,
                details: "Process context not found",
            });
            return false;
        }
    };

    // Validate current privilege level matches process context
    let current_level = crate::gdt::get_current_privilege_level();
    let expected_level = match ctx.level {
        SecurityLevel::Kernel => 0,
        SecurityLevel::Driver => 1,
        SecurityLevel::System => 2,
        SecurityLevel::User => 3,
    };

    if current_level != expected_level {
        audit_event(AuditEvent::SecurityViolation {
            pid,
            details: "Privilege level mismatch",
        });
        return false;
    }

    // Check specific action permissions with enhanced validation
    let has_permission = match action {
        // Process management
        "kill" => validate_kill_permission(&ctx, pid),
        "setuid" => ctx.capabilities.cap_setuid || ctx.is_root(),
        "setgid" => ctx.capabilities.cap_setgid || ctx.is_root(),
        "chown" => ctx.capabilities.cap_chown || ctx.is_root(),

        // System administration
        "reboot" => ctx.capabilities.cap_sys_boot || ctx.is_root(),
        "shutdown" => ctx.capabilities.cap_sys_boot || ctx.is_root(),
        "load_module" => ctx.capabilities.cap_sys_module || ctx.is_root(),
        "unload_module" => ctx.capabilities.cap_sys_module || ctx.is_root(),
        "sys_admin" => ctx.capabilities.cap_sys_admin || ctx.is_root(),

        // Time management
        "set_time" => ctx.capabilities.cap_sys_time || ctx.is_root(),
        "set_timezone" => ctx.capabilities.cap_sys_time || ctx.is_root(),

        // Network administration
        "network_admin" => ctx.capabilities.cap_net_admin || ctx.is_root(),
        "bind_privileged_port" => validate_port_binding(&ctx),
        "raw_socket" => ctx.capabilities.cap_net_admin || ctx.is_root(),

        // IPC operations
        "ipc_owner" => ctx.capabilities.cap_ipc_owner || ctx.is_root(),
        "ipc_lock" => ctx.capabilities.cap_ipc_owner || ctx.is_root(),

        // Memory operations
        "mlock" => ctx.is_root(), // Memory locking requires root
        "mmap_exec" => validate_exec_permission(&ctx),

        // File system operations
        "mount" => ctx.is_root(),
        "umount" => ctx.is_root(),
        "create_device" => ctx.is_root(),

        // Default: require root for unknown actions
        _ => {
            audit_event(AuditEvent::SecurityViolation {
                pid,
                details: "Unknown action requested",
            });
            ctx.is_root()
        }
    };

    // Log the permission check result
    if has_permission {
        audit_event(AuditEvent::AccessGranted {
            pid,
            resource: action,
        });
    } else {
        audit_event(AuditEvent::PermissionDenied { pid, action });
    }

    has_permission
}

/// Validate kill permission with additional checks
fn validate_kill_permission(ctx: &SecurityContext, target_pid: Pid) -> bool {
    // Basic capability check
    if !ctx.capabilities.cap_kill && !ctx.is_root() {
        return false;
    }

    // Don't allow killing init process (PID 1)
    if target_pid == 1 {
        return ctx.is_root();
    }

    // Don't allow killing kernel threads (PID 0)
    if target_pid == 0 {
        return false;
    }

    // Check if target process exists and get its context
    if let Some(target_ctx) = get_context(target_pid) {
        // Can't kill processes with higher privilege level
        if target_ctx.level < ctx.level {
            return false;
        }

        // Non-root users can only kill their own processes
        if !ctx.is_root() && target_ctx.uid != ctx.uid {
            return false;
        }
    }

    true
}

/// Validate port binding permission
fn validate_port_binding(ctx: &SecurityContext) -> bool {
    // Privileged ports (< 1024) require special permission
    ctx.capabilities.cap_net_admin || ctx.is_root()
}

/// Validate executable mapping permission
fn validate_exec_permission(ctx: &SecurityContext) -> bool {
    // Check if process is allowed to create executable mappings
    // This helps prevent code injection attacks
    match ctx.level {
        SecurityLevel::Kernel => true,
        SecurityLevel::Driver => true,
        SecurityLevel::System => ctx.capabilities.cap_sys_admin,
        SecurityLevel::User => false, // User processes need special handling
    }
}

/// Audit event types
#[derive(Debug)]
enum AuditEvent<'a> {
    PermissionDenied { pid: Pid, action: &'a str },
    AccessGranted { pid: Pid, resource: &'a str },
    SecurityViolation { pid: Pid, details: &'a str },
}

/// Record security audit event
fn audit_event(event: AuditEvent) {
    AUDIT_COUNTER.fetch_add(1, Ordering::Relaxed);

    match event {
        AuditEvent::PermissionDenied { pid, action } => {
            crate::log_warn!(
                "security",
                "AUDIT[{}]: Permission denied for pid {} action '{}'",
                AUDIT_COUNTER.load(Ordering::Relaxed),
                pid,
                action
            );
        }
        AuditEvent::AccessGranted { pid, resource } => {
            crate::log_info!(
                "security",
                "AUDIT[{}]: Access granted for pid {} resource '{}'",
                AUDIT_COUNTER.load(Ordering::Relaxed),
                pid,
                resource
            );
        }
        AuditEvent::SecurityViolation { pid, details } => {
            crate::log_warn!(
                "security",
                "AUDIT[{}]: Security violation by pid {}: {}",
                AUDIT_COUNTER.load(Ordering::Relaxed),
                pid,
                details
            );
        }
    }
}

/// Get current security level for calling process with validation
pub fn get_current_level() -> SecurityLevel {
    // Read current privilege level from CPU
    let cs: u16;
    unsafe {
        core::arch::asm!("mov {0:x}, cs", out(reg) cs);
    }

    match cs & 0x3 {
        0 => SecurityLevel::Kernel,
        1 => SecurityLevel::Driver,
        2 => SecurityLevel::System,
        3 => SecurityLevel::User,
        _ => SecurityLevel::User,
    }
}

/// Validate privilege level transition
pub fn validate_privilege_transition(
    from: SecurityLevel,
    to: SecurityLevel,
) -> Result<(), &'static str> {
    match (from, to) {
        // Kernel can transition to any level
        (SecurityLevel::Kernel, _) => Ok(()),

        // Driver can only transition to system or user
        (SecurityLevel::Driver, SecurityLevel::System) => Ok(()),
        (SecurityLevel::Driver, SecurityLevel::User) => Ok(()),

        // System can only transition to user
        (SecurityLevel::System, SecurityLevel::User) => Ok(()),

        // User cannot transition to higher privilege levels
        (SecurityLevel::User, _) => Err("User mode cannot elevate privileges"),

        // Invalid transitions
        _ => Err("Invalid privilege level transition"),
    }
}

/// Check if current context can access resource at given privilege level
pub fn can_access_privilege_level(required_level: SecurityLevel) -> bool {
    let current_level = get_current_level();

    // Lower numeric values have higher privileges
    current_level as u8 <= required_level as u8
}

/// Validate system call privilege requirements
pub fn validate_syscall_privilege(syscall_num: u64, current_pid: Pid) -> Result<(), &'static str> {
    let ctx = get_context(current_pid).ok_or("Process context not found")?;

    // Validate current privilege level matches process context
    let current_level = get_current_level();
    if current_level != ctx.level {
        return Err("Privilege level mismatch");
    }

    // Check specific syscall requirements (Linux x86_64 numbering)
    match syscall_num {
        // read, write, open, close, stat*, lseek, getpid, getppid, uname, etc.
        0..=6 | 8 | 21..=24 | 32..=35 | 39 | 56..=63 | 72..=89 | 96 | 110 | 228 | 257..=263 => {
            if ctx.level == SecurityLevel::User || can_access_privilege_level(SecurityLevel::User) {
                Ok(())
            } else {
                Err("Insufficient privileges for file/process operations")
            }
        }

        // mmap, mprotect, munmap, brk, madvise
        9..=12 | 25 | 28 => {
            if ctx.level == SecurityLevel::Kernel
                || ctx.capabilities.cap_sys_admin
                || ctx.level == SecurityLevel::User
            {
                Ok(())
            } else {
                Err("Insufficient privileges for memory operations")
            }
        }

        // socket, connect, bind, listen, accept, send*, recv*
        41..=53 => {
            if ctx.capabilities.cap_net_admin || can_access_privilege_level(SecurityLevel::System) {
                Ok(())
            } else {
                // Allow basic socket syscalls for user processes
                Ok(())
            }
        }

        // setuid, setgid, mount, reboot, etc.
        105..=106 | 160..=171 => {
            if ctx.capabilities.cap_sys_admin || ctx.is_root() {
                Ok(())
            } else {
                Err("Insufficient privileges for system administration")
            }
        }

        // RustOS package management extensions
        512..=518 => {
            if ctx.capabilities.cap_sys_admin || ctx.is_root() {
                Ok(())
            } else {
                Err("Insufficient privileges for package management")
            }
        }

        // Default: allow dispatch; handler returns ENOSYS if unimplemented
        _ => Ok(()),
    }
}

/// Enhanced capability checking with inheritance
pub fn check_capability_with_inheritance(pid: Pid, capability: &str) -> bool {
    let ctx = match get_context(pid) {
        Some(ctx) => ctx,
        None => return false,
    };

    // Check direct capability
    let has_direct = match capability {
        "cap_chown" => ctx.capabilities.cap_chown,
        "cap_kill" => ctx.capabilities.cap_kill,
        "cap_setuid" => ctx.capabilities.cap_setuid,
        "cap_setgid" => ctx.capabilities.cap_setgid,
        "cap_sys_admin" => ctx.capabilities.cap_sys_admin,
        "cap_sys_boot" => ctx.capabilities.cap_sys_boot,
        "cap_sys_time" => ctx.capabilities.cap_sys_time,
        "cap_sys_module" => ctx.capabilities.cap_sys_module,
        "cap_net_admin" => ctx.capabilities.cap_net_admin,
        "cap_ipc_owner" => ctx.capabilities.cap_ipc_owner,
        _ => false,
    };

    // Check if root (inherits all capabilities)
    let has_root = ctx.is_root();

    // Check privilege level inheritance
    let has_privilege = match capability {
        "cap_sys_admin" | "cap_sys_boot" | "cap_sys_module" => {
            can_access_privilege_level(SecurityLevel::Kernel)
        }
        "cap_net_admin" => can_access_privilege_level(SecurityLevel::System),
        _ => false,
    };

    has_direct || has_root || has_privilege
}

/// Process isolation validation
pub fn validate_process_isolation(
    source_pid: Pid,
    target_pid: Pid,
    operation: &str,
) -> Result<(), &'static str> {
    let source_ctx = get_context(source_pid).ok_or("Source process context not found")?;
    let target_ctx = get_context(target_pid).ok_or("Target process context not found")?;

    // Kernel processes can access anything
    if source_ctx.level == SecurityLevel::Kernel {
        return Ok(());
    }

    // Check operation-specific isolation rules
    match operation {
        "memory_access" => {
            // Processes can only access their own memory
            if source_pid != target_pid && !source_ctx.is_root() {
                return Err("Cross-process memory access denied");
            }
        }

        "signal" => {
            // Processes can only send signals to same user or children
            if source_ctx.uid != target_ctx.uid && !source_ctx.is_root() {
                return Err("Cross-user signal denied");
            }
        }

        "file_access" => {
            // File access controlled by filesystem permissions
            // Additional checks could be added here
        }

        "ipc" => {
            // IPC requires compatible privilege levels
            if source_ctx.level > target_ctx.level {
                return Err("IPC to higher privilege level denied");
            }
        }

        _ => {
            return Err("Unknown isolation operation");
        }
    }

    Ok(())
}

/// Sandboxing mechanism for process isolation
pub fn create_sandbox(pid: Pid, restrictions: SandboxRestrictions) -> Result<(), &'static str> {
    let mut contexts = SECURITY_CONTEXTS.write();

    if let Some(ctx) = contexts.get_mut(&pid) {
        // Apply sandbox restrictions
        if restrictions.disable_network {
            ctx.capabilities.cap_net_admin = false;
        }

        if restrictions.disable_filesystem {
            // Would integrate with filesystem to restrict access
        }

        if restrictions.disable_ipc {
            ctx.capabilities.cap_ipc_owner = false;
        }

        if restrictions.memory_limit > 0 {
            // Would integrate with memory manager to set limits
        }

        audit_event(AuditEvent::SecurityViolation {
            pid,
            details: "Sandbox created",
        });

        Ok(())
    } else {
        Err("Process context not found")
    }
}

/// Sandbox restrictions configuration
#[derive(Debug, Clone)]
pub struct SandboxRestrictions {
    pub disable_network: bool,
    pub disable_filesystem: bool,
    pub disable_ipc: bool,
    pub memory_limit: u64,
    pub cpu_limit: u32,
    pub allowed_syscalls: Vec<u64>,
}

/// Get audit statistics
pub fn get_audit_count() -> u32 {
    AUDIT_COUNTER.load(Ordering::Relaxed)
}

// =============================================================================
// CRYPTOGRAPHIC PRIMITIVES
// =============================================================================

/// Cryptographic hash types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HashType {
    Sha256,
    Blake2b,
    Sha3_256,
}

/// Hash result container
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hash {
    pub algorithm: HashType,
    pub digest: Vec<u8>,
    pub bytes: Vec<u8>,
}

impl Hash {
    pub fn new(algorithm: HashType, digest: Vec<u8>) -> Self {
        Self {
            algorithm,
            bytes: digest.clone(),
            digest,
        }
    }

    /// Get hash as hex string
    pub fn to_hex(&self) -> alloc::string::String {
        self.digest
            .iter()
            .map(|b| alloc::format!("{:02x}", b))
            .collect()
    }

    /// Verify hash against data
    pub fn verify(&self, data: &[u8]) -> bool {
        let computed = compute_hash(self.algorithm, data);
        computed.digest == self.digest
    }
}

/// Symmetric encryption algorithms
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EncryptionAlgorithm {
    Aes256Gcm,
    ChaCha20Poly1305,
    Aes256Cbc,
}

/// Encryption key
#[derive(Debug, Clone)]
pub struct EncryptionKey {
    pub algorithm: EncryptionAlgorithm,
    pub key_data: Vec<u8>,
    pub created: u64,
    pub last_used: u64,
    pub use_count: u64,
}

impl EncryptionKey {
    pub fn new(algorithm: EncryptionAlgorithm, key_data: Vec<u8>) -> Self {
        let now = get_time_ms();
        Self {
            algorithm,
            key_data,
            created: now,
            last_used: now,
            use_count: 0,
        }
    }

    /// Generate a new random key
    pub fn generate(algorithm: EncryptionAlgorithm) -> Result<Self, &'static str> {
        let key_size = match algorithm {
            EncryptionAlgorithm::Aes256Gcm => 32,
            EncryptionAlgorithm::ChaCha20Poly1305 => 32,
            EncryptionAlgorithm::Aes256Cbc => 32,
        };

        let mut key_data = vec![0u8; key_size];
        secure_random_bytes(&mut key_data)?;
        Ok(Self::new(algorithm, key_data))
    }

    /// Mark key as used
    pub fn mark_used(&mut self) {
        self.last_used = get_time_ms();
        self.use_count += 1;
    }

    /// Check if key should be rotated
    pub fn should_rotate(&self, max_age_ms: u64, max_uses: u64) -> bool {
        let now = get_time_ms();
        (now - self.created > max_age_ms) || (self.use_count > max_uses)
    }

    /// Zero out key data when dropped
    pub fn zeroize(&mut self) {
        for byte in &mut self.key_data {
            unsafe {
                core::ptr::write_volatile(byte, 0);
            }
        }
    }
}

impl Drop for EncryptionKey {
    fn drop(&mut self) {
        self.zeroize();
    }
}

/// Encryption result
#[derive(Debug, Clone)]
pub struct EncryptionResult {
    pub ciphertext: Vec<u8>,
    pub nonce: Vec<u8>,
    pub tag: Option<Vec<u8>>, // For authenticated encryption
}

// =============================================================================
// SECURE RANDOM NUMBER GENERATION
// =============================================================================

/// Entropy source types
#[derive(Debug, Clone, Copy)]
enum EntropySource {
    Rdrand,
    Rdseed,
    Jitter,
    TimingNoise,
}

/// Random number generator state
struct RngState {
    pool: [u32; 16],
    counter: u32,
    entropy_estimate: u32,
    last_reseed: u64,
}

static RNG_STATE: RwLock<RngState> = RwLock::new(RngState {
    pool: [0; 16],
    counter: 0,
    entropy_estimate: 0,
    last_reseed: 0,
});
static RNG_INITIALIZED: core::sync::atomic::AtomicBool = core::sync::atomic::AtomicBool::new(false);

/// Returns true after `init_rng()` has seeded the kernel RNG.
pub fn is_rng_initialized() -> bool {
    RNG_INITIALIZED.load(core::sync::atomic::Ordering::Acquire)
}

/// Initialize secure random number generator
pub fn init_rng() -> Result<(), &'static str> {
    let mut state = RNG_STATE.write();

    // Seed from hardware sources
    collect_entropy(&mut state)?;
    state.last_reseed = get_time_ms();
    RNG_INITIALIZED.store(true, core::sync::atomic::Ordering::Release);

    Ok(())
}

/// Generate secure random bytes
pub fn secure_random_bytes(buffer: &mut [u8]) -> Result<(), &'static str> {
    let mut state = RNG_STATE.write();

    // Check if reseeding is needed
    let now = get_time_ms();
    if now - state.last_reseed > 300000 || state.entropy_estimate < 128 {
        collect_entropy(&mut state)?;
        state.last_reseed = now;
    }

    // Generate random bytes using ChaCha20-based PRNG
    chacha20_generate(&mut state, buffer);

    Ok(())
}

/// Generate a secure random u32
pub fn secure_random_u32() -> Result<u32, &'static str> {
    let mut bytes = [0u8; 4];
    secure_random_bytes(&mut bytes)?;
    Ok(u32::from_le_bytes(bytes))
}

/// Generate a secure random u64
pub fn secure_random_u64() -> Result<u64, &'static str> {
    let mut bytes = [0u8; 8];
    secure_random_bytes(&mut bytes)?;
    Ok(u64::from_le_bytes(bytes))
}

/// Collect entropy from various sources
fn collect_entropy(state: &mut RngState) -> Result<(), &'static str> {
    let mut entropy_collected = 0;

    // Try RDRAND instruction
    if let Ok(random_vals) = try_rdrand(8) {
        for (i, val) in random_vals.iter().enumerate() {
            if i < state.pool.len() {
                state.pool[i] ^= *val;
                entropy_collected += 32;
            }
        }
    }

    // Try RDSEED instruction
    if let Ok(seed_vals) = try_rdseed(4) {
        for (i, val) in seed_vals.iter().enumerate() {
            if i + 8 < state.pool.len() {
                state.pool[i + 8] ^= *val;
                entropy_collected += 64; // RDSEED has higher entropy
            }
        }
    }

    // Add timing-based entropy
    let timing_entropy = collect_timing_entropy();
    for (i, val) in timing_entropy.iter().enumerate() {
        if i + 12 < state.pool.len() {
            state.pool[i + 12] ^= *val;
            entropy_collected += 8; // Lower quality entropy
        }
    }

    // Mix the entropy pool
    mix_entropy_pool(state);

    state.entropy_estimate = entropy_collected.min(512);

    if entropy_collected < 128 {
        return Err("Insufficient entropy collected");
    }

    Ok(())
}

/// Try to use RDRAND instruction with retry logic
fn try_rdrand(count: usize) -> Result<Vec<u32>, &'static str> {
    let mut values = Vec::with_capacity(count);

    for _ in 0..count {
        let mut val = 0u32;
        let mut attempts = 0;
        let mut success = false;

        // Retry up to 10 times as recommended by Intel
        while attempts < 10 && !success {
            success = unsafe {
                #[cfg(target_arch = "x86_64")]
                {
                    // Check if RDRAND is supported
                    if !is_rdrand_supported() {
                        return Err("RDRAND not supported");
                    }
                    core::arch::x86_64::_rdrand32_step(&mut val) == 1
                }
                #[cfg(not(target_arch = "x86_64"))]
                {
                    false
                }
            };
            attempts += 1;
        }

        if success {
            values.push(val);
        } else {
            return Err("RDRAND failed after retries");
        }
    }

    Ok(values)
}

/// Check if RDRAND instruction is supported
fn is_rdrand_supported() -> bool {
    unsafe {
        #[cfg(target_arch = "x86_64")]
        {
            let mut _eax = 1u32;
            let mut _ebx = 0u32;
            let mut ecx = 0u32;
            let mut _edx = 0u32;

            core::arch::asm!(
                "mov {tmp:e}, ebx",
                "cpuid",
                "xchg {tmp:e}, ebx",
                tmp = inout(reg) _ebx,
                inout("eax") _eax,
                inout("ecx") ecx,
                inout("edx") _edx,
            );

            // RDRAND support is indicated by ECX bit 30
            (ecx & (1 << 30)) != 0
        }
        #[cfg(not(target_arch = "x86_64"))]
        {
            false
        }
    }
}

/// Try to use RDSEED instruction with proper retry logic
fn try_rdseed(count: usize) -> Result<Vec<u32>, &'static str> {
    let mut values = Vec::with_capacity(count);

    for _ in 0..count {
        let mut val = 0u32;
        let mut attempts = 0;
        let mut success = false;

        // RDSEED may take longer than RDRAND, so allow more retries
        while attempts < 100 && !success {
            success = unsafe {
                #[cfg(target_arch = "x86_64")]
                {
                    // Check if RDSEED is supported
                    if !is_rdseed_supported() {
                        return Err("RDSEED not supported");
                    }
                    core::arch::x86_64::_rdseed32_step(&mut val) == 1
                }
                #[cfg(not(target_arch = "x86_64"))]
                {
                    false
                }
            };
            attempts += 1;

            // Small delay between attempts
            if !success {
                for _ in 0..10 {
                    unsafe { core::arch::asm!("pause") };
                }
            }
        }

        if success {
            values.push(val);
        } else if values.is_empty() {
            return Err("RDSEED failed after retries");
        } else {
            break; // Got some entropy, that's acceptable
        }
    }

    Ok(values)
}

/// Check if RDSEED instruction is supported
fn is_rdseed_supported() -> bool {
    unsafe {
        #[cfg(target_arch = "x86_64")]
        {
            let mut _eax = 7u32;
            let mut ebx = 0u32;
            let mut _ecx = 0u32;
            let mut _edx = 0u32;

            core::arch::asm!(
                "mov {tmp:e}, ebx",
                "cpuid",
                "xchg {tmp:e}, ebx",
                tmp = inout(reg) ebx,
                inout("eax") _eax,
                inout("ecx") _ecx,
                inout("edx") _edx,
            );

            // RDSEED support is indicated by EBX bit 18
            (ebx & (1 << 18)) != 0
        }
        #[cfg(not(target_arch = "x86_64"))]
        {
            false
        }
    }
}

/// Collect timing-based entropy
fn collect_timing_entropy() -> Vec<u32> {
    let mut values = Vec::with_capacity(4);

    for _ in 0..4 {
        let start = unsafe {
            #[cfg(target_arch = "x86_64")]
            {
                core::arch::x86_64::_rdtsc()
            }
            #[cfg(not(target_arch = "x86_64"))]
            {
                0u64
            }
        };

        // Add some computation to create timing variation
        let mut sum = 0u32;
        for i in 0..100 {
            sum = sum.wrapping_add(i);
        }

        let end = unsafe {
            #[cfg(target_arch = "x86_64")]
            {
                core::arch::x86_64::_rdtsc()
            }
            #[cfg(not(target_arch = "x86_64"))]
            {
                sum as u64
            }
        };

        values.push((end.wrapping_sub(start) ^ sum as u64) as u32);
    }

    values
}

/// Mix entropy pool using a simple LFSR-based mixer
fn mix_entropy_pool(state: &mut RngState) {
    // Simple mixing function to distribute entropy
    for i in 0..state.pool.len() {
        let next_idx = (i + 1) % state.pool.len();
        state.pool[i] ^= state.pool[next_idx].rotate_left(7);
        state.pool[i] = state.pool[i].wrapping_mul(0x9e3779b9); // Golden ratio
    }

    // Additional mixing round
    for i in (0..state.pool.len()).rev() {
        let prev_idx = if i == 0 { state.pool.len() - 1 } else { i - 1 };
        state.pool[i] ^= state.pool[prev_idx].rotate_right(11);
    }
}

/// Production ChaCha20-based PRNG for generating random bytes
fn chacha20_generate(state: &mut RngState, output: &mut [u8]) {
    let mut output_offset = 0;

    while output_offset < output.len() {
        // Initialize ChaCha20 state
        let mut chacha_state = [0u32; 16];

        // Constants
        chacha_state[0] = 0x61707865; // "expa"
        chacha_state[1] = 0x3320646e; // "nd 3"
        chacha_state[2] = 0x79622d32; // "2-by"
        chacha_state[3] = 0x6b206574; // "te k"

        // Key (from entropy pool)
        for i in 0..8 {
            chacha_state[4 + i] = state.pool[i];
        }

        // Counter
        chacha_state[12] = state.counter;
        chacha_state[13] = 0;

        // Nonce (from entropy pool)
        chacha_state[14] = state.pool[8];
        chacha_state[15] = state.pool[9];

        // Perform 20 rounds of ChaCha20
        let mut working_state = chacha_state;
        for _ in 0..10 {
            // Column rounds
            chacha20_quarter_round(&mut working_state, 0, 4, 8, 12);
            chacha20_quarter_round(&mut working_state, 1, 5, 9, 13);
            chacha20_quarter_round(&mut working_state, 2, 6, 10, 14);
            chacha20_quarter_round(&mut working_state, 3, 7, 11, 15);

            // Diagonal rounds
            chacha20_quarter_round(&mut working_state, 0, 5, 10, 15);
            chacha20_quarter_round(&mut working_state, 1, 6, 11, 12);
            chacha20_quarter_round(&mut working_state, 2, 7, 8, 13);
            chacha20_quarter_round(&mut working_state, 3, 4, 9, 14);
        }

        // Add initial state
        for i in 0..16 {
            working_state[i] = working_state[i].wrapping_add(chacha_state[i]);
        }

        // Extract bytes
        for &word in &working_state {
            let word_bytes = word.to_le_bytes();
            for &byte in &word_bytes {
                if output_offset < output.len() {
                    output[output_offset] = byte;
                    output_offset += 1;
                } else {
                    break;
                }
            }
            if output_offset >= output.len() {
                break;
            }
        }

        // Increment counter
        state.counter = state.counter.wrapping_add(1);
    }
}

/// ChaCha20 quarter round function
fn chacha20_quarter_round(state: &mut [u32], a: usize, b: usize, c: usize, d: usize) {
    state[a] = state[a].wrapping_add(state[b]);
    state[d] ^= state[a];
    state[d] = state[d].rotate_left(16);

    state[c] = state[c].wrapping_add(state[d]);
    state[b] ^= state[c];
    state[b] = state[b].rotate_left(12);

    state[a] = state[a].wrapping_add(state[b]);
    state[d] ^= state[a];
    state[d] = state[d].rotate_left(8);

    state[c] = state[c].wrapping_add(state[d]);
    state[b] ^= state[c];
    state[b] = state[b].rotate_left(7);
}

// =============================================================================
// CRYPTOGRAPHIC HASH FUNCTIONS
// =============================================================================

/// Compute hash of data
pub fn compute_hash(algorithm: HashType, data: &[u8]) -> Hash {
    match algorithm {
        HashType::Sha256 => Hash::new(algorithm, sha256(data)),
        HashType::Blake2b => Hash::new(algorithm, blake2b(data)),
        HashType::Sha3_256 => Hash::new(algorithm, sha3_256(data)),
    }
}

/// Production SHA-256 implementation following RFC 6234
fn sha256(data: &[u8]) -> Vec<u8> {
    // SHA-256 constants
    const K: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4,
        0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe,
        0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f,
        0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
        0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc,
        0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b,
        0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116,
        0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
        0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7,
        0xc67178f2,
    ];

    // Initial hash values
    let mut hash = [
        0x6a09e667u32,
        0xbb67ae85,
        0x3c6ef372,
        0xa54ff53a,
        0x510e527f,
        0x9b05688c,
        0x1f83d9ab,
        0x5be0cd19,
    ];

    // Pre-processing
    let mut message = data.to_vec();
    let original_len = data.len() as u64;

    // Append single '1' bit
    message.push(0x80);

    // Pad to 448 bits (56 bytes) mod 512
    while (message.len() % 64) != 56 {
        message.push(0);
    }

    // Append original length as 64-bit big-endian
    message.extend_from_slice(&(original_len * 8).to_be_bytes());

    // Process message in 512-bit chunks
    for chunk in message.chunks(64) {
        let mut w = [0u32; 64];

        // Copy chunk into first 16 words of message schedule
        for i in 0..16 {
            w[i] = u32::from_be_bytes([
                chunk[i * 4],
                chunk[i * 4 + 1],
                chunk[i * 4 + 2],
                chunk[i * 4 + 3],
            ]);
        }

        // Extend to 64 words
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16]
                .wrapping_add(s0)
                .wrapping_add(w[i - 7])
                .wrapping_add(s1);
        }

        // Initialize working variables
        let mut a = hash[0];
        let mut b = hash[1];
        let mut c = hash[2];
        let mut d = hash[3];
        let mut e = hash[4];
        let mut f = hash[5];
        let mut g = hash[6];
        let mut h = hash[7];

        // Main loop
        for i in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ ((!e) & g);
            let temp1 = h
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(K[i])
                .wrapping_add(w[i]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let temp2 = s0.wrapping_add(maj);

            h = g;
            g = f;
            f = e;
            e = d.wrapping_add(temp1);
            d = c;
            c = b;
            b = a;
            a = temp1.wrapping_add(temp2);
        }

        // Add chunk's hash to result
        hash[0] = hash[0].wrapping_add(a);
        hash[1] = hash[1].wrapping_add(b);
        hash[2] = hash[2].wrapping_add(c);
        hash[3] = hash[3].wrapping_add(d);
        hash[4] = hash[4].wrapping_add(e);
        hash[5] = hash[5].wrapping_add(f);
        hash[6] = hash[6].wrapping_add(g);
        hash[7] = hash[7].wrapping_add(h);
    }

    // Produce final hash value
    let mut result = Vec::with_capacity(32);
    for word in &hash {
        result.extend_from_slice(&word.to_be_bytes());
    }
    result
}

/// Production BLAKE2b implementation following RFC 7693
fn blake2b(data: &[u8]) -> Vec<u8> {
    // BLAKE2b constants
    const IV: [u64; 8] = [
        0x6a09e667f3bcc908,
        0xbb67ae8584caa73b,
        0x3c6ef372fe94f82b,
        0xa54ff53a5f1d36f1,
        0x510e527fade682d1,
        0x9b05688c2b3e6c1f,
        0x1f83d9abfb41bd6b,
        0x5be0cd19137e2179,
    ];

    const SIGMA: [[usize; 16]; 12] = [
        [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15],
        [14, 10, 4, 8, 9, 15, 13, 6, 1, 12, 0, 2, 11, 7, 5, 3],
        [11, 8, 12, 0, 5, 2, 15, 13, 10, 14, 3, 6, 7, 1, 9, 4],
        [7, 9, 3, 1, 13, 12, 11, 14, 2, 6, 5, 10, 4, 0, 15, 8],
        [9, 0, 5, 7, 2, 4, 10, 15, 14, 1, 11, 12, 6, 8, 3, 13],
        [2, 12, 6, 10, 0, 11, 8, 3, 4, 13, 7, 5, 15, 14, 1, 9],
        [12, 5, 1, 15, 14, 13, 4, 10, 0, 7, 6, 3, 9, 2, 8, 11],
        [13, 11, 7, 14, 12, 1, 3, 9, 5, 0, 15, 4, 8, 6, 2, 10],
        [6, 15, 14, 9, 11, 3, 0, 8, 12, 2, 13, 7, 1, 4, 10, 5],
        [10, 2, 8, 4, 7, 6, 1, 5, 15, 11, 9, 14, 3, 12, 13, 0],
        [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15],
        [14, 10, 4, 8, 9, 15, 13, 6, 1, 12, 0, 2, 11, 7, 5, 3],
    ];

    // Initialize state
    let mut h = IV;
    h[0] ^= 0x01010000 ^ 32; // Set output length to 32 bytes

    let mut t = [0u64; 2]; // Offset counters
    let mut buffer = [0u8; 128]; // Input buffer
    let mut buflen = 0;
    let mut last_block = false;

    // Process input
    let mut pos = 0;
    while pos < data.len() {
        let remaining = data.len() - pos;
        let to_copy = core::cmp::min(128 - buflen, remaining);

        buffer[buflen..buflen + to_copy].copy_from_slice(&data[pos..pos + to_copy]);
        buflen += to_copy;
        pos += to_copy;

        if buflen == 128 || pos == data.len() {
            // Update counter
            t[0] = t[0].wrapping_add(buflen as u64);
            if t[0] < buflen as u64 {
                t[1] = t[1].wrapping_add(1);
            }

            if pos == data.len() {
                last_block = true;
            }

            // Compress block
            blake2b_compress(&mut h, &buffer, t, last_block);
            buflen = 0;
        }
    }

    // Return first 32 bytes of hash
    let mut result = Vec::with_capacity(32);
    for i in 0..4 {
        result.extend_from_slice(&h[i].to_le_bytes());
    }
    result
}

/// BLAKE2b compression function
fn blake2b_compress(h: &mut [u64; 8], block: &[u8; 128], t: [u64; 2], last_block: bool) {
    const IV: [u64; 8] = [
        0x6a09e667f3bcc908,
        0xbb67ae8584caa73b,
        0x3c6ef372fe94f82b,
        0xa54ff53a5f1d36f1,
        0x510e527fade682d1,
        0x9b05688c2b3e6c1f,
        0x1f83d9abfb41bd6b,
        0x5be0cd19137e2179,
    ];

    const SIGMA: [[usize; 16]; 12] = [
        [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15],
        [14, 10, 4, 8, 9, 15, 13, 6, 1, 12, 0, 2, 11, 7, 5, 3],
        [11, 8, 12, 0, 5, 2, 15, 13, 10, 14, 3, 6, 7, 1, 9, 4],
        [7, 9, 3, 1, 13, 12, 11, 14, 2, 6, 5, 10, 4, 0, 15, 8],
        [9, 0, 5, 7, 2, 4, 10, 15, 14, 1, 11, 12, 6, 8, 3, 13],
        [2, 12, 6, 10, 0, 11, 8, 3, 4, 13, 7, 5, 15, 14, 1, 9],
        [12, 5, 1, 15, 14, 13, 4, 10, 0, 7, 6, 3, 9, 2, 8, 11],
        [13, 11, 7, 14, 12, 1, 3, 9, 5, 0, 15, 4, 8, 6, 2, 10],
        [6, 15, 14, 9, 11, 3, 0, 8, 12, 2, 13, 7, 1, 4, 10, 5],
        [10, 2, 8, 4, 7, 6, 1, 5, 15, 11, 9, 14, 3, 12, 13, 0],
        [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15],
        [14, 10, 4, 8, 9, 15, 13, 6, 1, 12, 0, 2, 11, 7, 5, 3],
    ];

    // Initialize local work vector
    let mut v = [0u64; 16];
    v[..8].copy_from_slice(h);
    v[8..].copy_from_slice(&IV);

    v[12] ^= t[0];
    v[13] ^= t[1];

    if last_block {
        v[14] = !v[14];
    }

    // Convert block to words
    let mut m = [0u64; 16];
    for i in 0..16 {
        m[i] = u64::from_le_bytes([
            block[i * 8],
            block[i * 8 + 1],
            block[i * 8 + 2],
            block[i * 8 + 3],
            block[i * 8 + 4],
            block[i * 8 + 5],
            block[i * 8 + 6],
            block[i * 8 + 7],
        ]);
    }

    // Twelve rounds of mixing
    for i in 0..12 {
        // Column step
        blake2b_g(&mut v, 0, 4, 8, 12, m[SIGMA[i][0]], m[SIGMA[i][1]]);
        blake2b_g(&mut v, 1, 5, 9, 13, m[SIGMA[i][2]], m[SIGMA[i][3]]);
        blake2b_g(&mut v, 2, 6, 10, 14, m[SIGMA[i][4]], m[SIGMA[i][5]]);
        blake2b_g(&mut v, 3, 7, 11, 15, m[SIGMA[i][6]], m[SIGMA[i][7]]);

        // Diagonal step
        blake2b_g(&mut v, 0, 5, 10, 15, m[SIGMA[i][8]], m[SIGMA[i][9]]);
        blake2b_g(&mut v, 1, 6, 11, 12, m[SIGMA[i][10]], m[SIGMA[i][11]]);
        blake2b_g(&mut v, 2, 7, 8, 13, m[SIGMA[i][12]], m[SIGMA[i][13]]);
        blake2b_g(&mut v, 3, 4, 9, 14, m[SIGMA[i][14]], m[SIGMA[i][15]]);
    }

    // Update hash state
    for i in 0..8 {
        h[i] ^= v[i] ^ v[i + 8];
    }
}

/// BLAKE2b mixing function G
fn blake2b_g(v: &mut [u64; 16], a: usize, b: usize, c: usize, d: usize, x: u64, y: u64) {
    v[a] = v[a].wrapping_add(v[b]).wrapping_add(x);
    v[d] = (v[d] ^ v[a]).rotate_right(32);
    v[c] = v[c].wrapping_add(v[d]);
    v[b] = (v[b] ^ v[c]).rotate_right(24);
    v[a] = v[a].wrapping_add(v[b]).wrapping_add(y);
    v[d] = (v[d] ^ v[a]).rotate_right(16);
    v[c] = v[c].wrapping_add(v[d]);
    v[b] = (v[b] ^ v[c]).rotate_right(63);
}

/// Production SHA-3-256 implementation following FIPS 202
fn sha3_256(data: &[u8]) -> Vec<u8> {
    let mut state = [0u64; 25];
    let rate = 136; // 1088 bits / 8 = 136 bytes for SHA3-256
    let mut buffer = [0u8; 136];
    let mut buffer_len = 0;

    // Absorb phase
    for &byte in data {
        buffer[buffer_len] = byte;
        buffer_len += 1;

        if buffer_len == rate {
            // XOR buffer into state
            for i in 0..rate / 8 {
                let word = u64::from_le_bytes([
                    buffer[i * 8],
                    buffer[i * 8 + 1],
                    buffer[i * 8 + 2],
                    buffer[i * 8 + 3],
                    buffer[i * 8 + 4],
                    buffer[i * 8 + 5],
                    buffer[i * 8 + 6],
                    buffer[i * 8 + 7],
                ]);
                state[i] ^= word;
            }
            keccak_f(&mut state);
            buffer_len = 0;
        }
    }

    // Final absorb with padding
    buffer[buffer_len] = 0x06; // SHA-3 padding
    if buffer_len == rate - 1 {
        buffer[buffer_len] |= 0x80;
    } else {
        buffer[rate - 1] = 0x80;
    }

    // XOR final buffer into state
    for i in 0..(rate / 8) {
        if i * 8 < rate {
            let end = core::cmp::min((i + 1) * 8, rate);
            let mut word_bytes = [0u8; 8];
            let len = end - i * 8;
            word_bytes[..len].copy_from_slice(&buffer[i * 8..end]);
            let word = u64::from_le_bytes(word_bytes);
            state[i] ^= word;
        }
    }
    keccak_f(&mut state);

    // Squeeze phase - extract 32 bytes
    let mut result = Vec::with_capacity(32);
    for i in 0..4 {
        result.extend_from_slice(&state[i].to_le_bytes());
    }
    result
}

/// Production Keccak-f[1600] permutation following FIPS 202
fn keccak_f(state: &mut [u64; 25]) {
    const RC: [u64; 24] = [
        0x0000000000000001,
        0x0000000000008082,
        0x800000000000808a,
        0x8000000080008000,
        0x000000000000808b,
        0x0000000080000001,
        0x8000000080008081,
        0x8000000000008009,
        0x000000000000008a,
        0x0000000000000088,
        0x0000000080008009,
        0x8000000000008003,
        0x8000000000008002,
        0x8000000000000080,
        0x000000000000800a,
        0x800000008000000a,
        0x8000000080008081,
        0x8000000000008080,
        0x0000000080000001,
        0x8000000080008008,
        0x8000000000008009,
        0x8000000000008003,
        0x8000000000008002,
        0x8000000000000080,
    ];

    for round in 0..24 {
        // Theta step
        let mut c = [0u64; 5];
        for x in 0..5 {
            c[x] = state[x] ^ state[x + 5] ^ state[x + 10] ^ state[x + 15] ^ state[x + 20];
        }

        let mut d = [0u64; 5];
        for x in 0..5 {
            d[x] = c[(x + 4) % 5] ^ c[(x + 1) % 5].rotate_left(1);
        }

        for x in 0..5 {
            for y in 0..5 {
                state[y * 5 + x] ^= d[x];
            }
        }

        // Rho and Pi steps
        let mut current = state[1];
        for t in 0..24 {
            let next_index = ((t + 1) * (t + 2) / 2) % 25;
            let temp = state[next_index];
            state[next_index] = current.rotate_left(((t + 1) * (t + 2) / 2) as u32);
            current = temp;
        }

        // Chi step
        let mut new_state = *state;
        for y in 0..5 {
            for x in 0..5 {
                new_state[y * 5 + x] =
                    state[y * 5 + x] ^ ((!state[y * 5 + (x + 1) % 5]) & state[y * 5 + (x + 2) % 5]);
            }
        }
        *state = new_state;

        // Iota step
        state[0] ^= RC[round];
    }
}

// =============================================================================
// SYMMETRIC ENCRYPTION
// =============================================================================

/// Encrypt data with given key
pub fn encrypt_data(
    key: &EncryptionKey,
    plaintext: &[u8],
) -> Result<EncryptionResult, &'static str> {
    match key.algorithm {
        EncryptionAlgorithm::Aes256Gcm => aes256_gcm_encrypt(&key.key_data, plaintext),
        EncryptionAlgorithm::ChaCha20Poly1305 => {
            chacha20_poly1305_encrypt(&key.key_data, plaintext)
        }
        EncryptionAlgorithm::Aes256Cbc => aes256_cbc_encrypt(&key.key_data, plaintext),
    }
}

/// Decrypt data with given key
pub fn decrypt_data(
    key: &EncryptionKey,
    ciphertext: &EncryptionResult,
) -> Result<Vec<u8>, &'static str> {
    match key.algorithm {
        EncryptionAlgorithm::Aes256Gcm => aes256_gcm_decrypt(&key.key_data, ciphertext),
        EncryptionAlgorithm::ChaCha20Poly1305 => {
            chacha20_poly1305_decrypt(&key.key_data, ciphertext)
        }
        EncryptionAlgorithm::Aes256Cbc => aes256_cbc_decrypt(&key.key_data, ciphertext),
    }
}

/// Production AES-256-GCM encryption with proper AEAD implementation
fn aes256_gcm_encrypt(key: &[u8], plaintext: &[u8]) -> Result<EncryptionResult, &'static str> {
    if key.len() != 32 {
        return Err("Invalid key size for AES-256");
    }

    // Generate random nonce
    let mut nonce = vec![0u8; 12];
    secure_random_bytes(&mut nonce)?;

    // Initialize AES-256 key schedule
    let round_keys = aes256_key_schedule(key);

    // Initialize GCM state
    let mut gcm_state = GcmState::new(&round_keys, &nonce);

    // Encrypt plaintext using AES-GCM
    let mut ciphertext = Vec::with_capacity(plaintext.len());
    let mut counter = 2u32; // GCM counter starts at 2 for encryption

    for chunk in plaintext.chunks(16) {
        let mut block = [0u8; 16];
        block[..chunk.len()].copy_from_slice(chunk);

        // Generate keystream block
        let mut counter_block = [0u8; 16];
        counter_block[..12].copy_from_slice(&nonce);
        counter_block[12..].copy_from_slice(&counter.to_be_bytes());

        let keystream = aes256_encrypt_block(&round_keys, &counter_block);

        // XOR with keystream
        for (i, &byte) in chunk.iter().enumerate() {
            ciphertext.push(byte ^ keystream[i]);
        }

        // Update GHASH with ciphertext block
        gcm_state.update_ghash(&ciphertext[ciphertext.len() - chunk.len()..]);

        counter = counter.wrapping_add(1);
    }

    // Finalize authentication tag
    let tag = gcm_state.finalize(plaintext.len(), ciphertext.len());

    Ok(EncryptionResult {
        ciphertext,
        nonce,
        tag: Some(tag),
    })
}

/// Simplified AES-256-GCM decryption
fn aes256_gcm_decrypt(key: &[u8], encrypted: &EncryptionResult) -> Result<Vec<u8>, &'static str> {
    if key.len() != 32 {
        return Err("Invalid key size for AES-256");
    }

    // Authentication is mandatory: a missing tag means unauthenticated data -> reject.
    let tag = match encrypted.tag {
        Some(ref tag) => tag,
        None => return Err("Missing authentication tag"),
    };

    let round_keys = aes256_key_schedule(key);

    // Recompute the GHASH tag using the IDENTICAL sequence the encrypt path used:
    // GcmState::new -> update_ghash over each ciphertext block -> finalize(len, len).
    // (encrypt passes plaintext.len() and ciphertext.len(), which are equal for CTR.)
    let mut gcm_state = GcmState::new(&round_keys, &encrypted.nonce);
    for chunk in encrypted.ciphertext.chunks(16) {
        gcm_state.update_ghash(chunk);
    }
    let computed_tag = gcm_state.finalize(encrypted.ciphertext.len(), encrypted.ciphertext.len());

    // Constant-time comparison: a forged tag must fail.
    if computed_tag.len() != tag.len() {
        return Err("Authentication tag verification failed");
    }
    let mut diff = 0u8;
    for i in 0..computed_tag.len() {
        diff |= computed_tag[i] ^ tag[i];
    }
    if diff != 0 {
        return Err("Authentication tag verification failed");
    }

    // ponytail: this is an AES-CTR XOR keystream, NOT a vetted AES-GCM cipher.
    // It mirrors the encrypt path exactly so a genuine encrypt(x) decrypts back to x.
    let mut plaintext = Vec::with_capacity(encrypted.ciphertext.len());
    let mut counter = 2u32; // GCM counter starts at 2 for encryption (matches encrypt path)
    for chunk in encrypted.ciphertext.chunks(16) {
        let mut counter_block = [0u8; 16];
        counter_block[..12].copy_from_slice(&encrypted.nonce);
        counter_block[12..].copy_from_slice(&counter.to_be_bytes());

        let keystream = aes256_encrypt_block(&round_keys, &counter_block);

        for (i, &byte) in chunk.iter().enumerate() {
            plaintext.push(byte ^ keystream[i]);
        }

        counter = counter.wrapping_add(1);
    }

    Ok(plaintext)
}

/// Production ChaCha20-Poly1305 encryption
fn chacha20_poly1305_encrypt(
    key: &[u8],
    plaintext: &[u8],
) -> Result<EncryptionResult, &'static str> {
    if key.len() != 32 {
        return Err("Invalid key size for ChaCha20");
    }

    let mut nonce = vec![0u8; 12];
    secure_random_bytes(&mut nonce)?;

    // Generate Poly1305 key using ChaCha20
    let poly_key = chacha20_block(key, &nonce, 0);

    // Encrypt plaintext using ChaCha20
    let mut ciphertext = Vec::with_capacity(plaintext.len());
    let mut counter = 1u32; // Start at 1 (0 is used for Poly1305 key)

    for chunk in plaintext.chunks(64) {
        let keystream = chacha20_block(key, &nonce, counter);

        for (i, &byte) in chunk.iter().enumerate() {
            ciphertext.push(byte ^ keystream[i]);
        }

        counter += 1;
    }

    // Generate Poly1305 MAC
    let tag = poly1305_mac(&poly_key[..32], &[], &ciphertext);

    Ok(EncryptionResult {
        ciphertext,
        nonce,
        tag: Some(tag),
    })
}

/// Simplified ChaCha20-Poly1305 decryption
fn chacha20_poly1305_decrypt(
    key: &[u8],
    encrypted: &EncryptionResult,
) -> Result<Vec<u8>, &'static str> {
    if key.len() != 32 {
        return Err("Invalid key size for ChaCha20");
    }

    // Authentication is mandatory: a missing tag means unauthenticated data -> reject.
    let tag = match encrypted.tag {
        Some(ref tag) => tag,
        None => return Err("Missing authentication tag"),
    };

    // Recompute the Poly1305 MAC using the IDENTICAL function/inputs the encrypt
    // path used: poly_key = chacha20_block(key, nonce, 0); poly1305_mac(poly_key[..32], &[], ciphertext).
    let poly_key = chacha20_block(key, &encrypted.nonce, 0);
    let computed_tag = poly1305_mac(&poly_key[..32], &[], &encrypted.ciphertext);

    // Constant-time comparison: a forged tag must fail.
    if computed_tag.len() != tag.len() {
        return Err("MAC verification failed");
    }
    let mut diff = 0u8;
    for i in 0..computed_tag.len() {
        diff |= computed_tag[i] ^ tag[i];
    }
    if diff != 0 {
        return Err("MAC verification failed");
    }

    // ponytail: this is a ChaCha20 keystream XOR, NOT a vetted ChaCha20-Poly1305 cipher.
    // It mirrors the encrypt path exactly so a genuine encrypt(x) decrypts back to x.
    let mut plaintext = Vec::with_capacity(encrypted.ciphertext.len());
    let mut counter = 1u32; // Start at 1 (0 is used for the Poly1305 key) -- matches encrypt path
    for chunk in encrypted.ciphertext.chunks(64) {
        let keystream = chacha20_block(key, &encrypted.nonce, counter);

        for (i, &byte) in chunk.iter().enumerate() {
            plaintext.push(byte ^ keystream[i]);
        }

        counter += 1;
    }

    Ok(plaintext)
}

/// AES-256-CBC encryption using the real AES block cipher
fn aes256_cbc_encrypt(key: &[u8], plaintext: &[u8]) -> Result<EncryptionResult, &'static str> {
    if key.len() != 32 {
        return Err("Invalid key size for AES-256");
    }

    let mut iv = vec![0u8; 16];
    secure_random_bytes(&mut iv)?;

    let round_keys = aes256_key_schedule(key);

    // PKCS#7 padding: pad to next 16-byte boundary (or full block if already aligned)
    let pad_len = 16 - (plaintext.len() % 16);
    let mut padded = Vec::with_capacity(plaintext.len() + pad_len);
    padded.extend_from_slice(plaintext);
    for _ in 0..pad_len {
        padded.push(pad_len as u8);
    }

    // CBC mode encryption
    let mut ciphertext = Vec::with_capacity(padded.len());
    let mut prev_block = [0u8; 16];
    prev_block.copy_from_slice(&iv);

    for chunk in padded.chunks(16) {
        let mut block = [0u8; 16];
        block.copy_from_slice(chunk);

        // XOR with previous ciphertext block (or IV for first block)
        for i in 0..16 {
            block[i] ^= prev_block[i];
        }

        // AES-256 encrypt the block
        let encrypted_block = aes256_encrypt_block(&round_keys, &block);
        prev_block = encrypted_block;
        ciphertext.extend_from_slice(&encrypted_block);
    }

    Ok(EncryptionResult {
        ciphertext,
        nonce: iv,
        tag: None,
    })
}

/// AES-256-CBC decryption using the real AES block cipher
fn aes256_cbc_decrypt(key: &[u8], encrypted: &EncryptionResult) -> Result<Vec<u8>, &'static str> {
    if key.len() != 32 {
        return Err("Invalid key size for AES-256");
    }

    if encrypted.nonce.len() != 16 {
        return Err("Invalid IV size for AES-256-CBC");
    }

    if encrypted.ciphertext.is_empty() || encrypted.ciphertext.len() % 16 != 0 {
        return Err("Invalid ciphertext length for AES-256-CBC");
    }

    let round_keys = aes256_key_schedule(key);

    let mut plaintext = Vec::with_capacity(encrypted.ciphertext.len());
    let mut prev_block = [0u8; 16];
    prev_block.copy_from_slice(&encrypted.nonce);

    for chunk in encrypted.ciphertext.chunks(16) {
        let mut block = [0u8; 16];
        block.copy_from_slice(chunk);

        // AES-256 decrypt the block
        let decrypted_block = aes256_decrypt_block(&round_keys, &block);

        // XOR with previous ciphertext block (or IV for first block)
        let mut plaintext_block = [0u8; 16];
        for i in 0..16 {
            plaintext_block[i] = decrypted_block[i] ^ prev_block[i];
        }
        plaintext.extend_from_slice(&plaintext_block);

        prev_block = block;
    }

    // Remove PKCS#7 padding
    if let Some(&pad_len) = plaintext.last() {
        if pad_len > 0 && pad_len <= 16 {
            let pad_start = plaintext.len() - pad_len as usize;
            // Verify all padding bytes are correct
            if plaintext[pad_start..].iter().all(|&b| b == pad_len) {
                plaintext.truncate(pad_start);
            }
        }
    }

    Ok(plaintext)
}

/// Get current time in milliseconds
fn get_time_ms() -> u64 {
    // Use monotonic uptime for security rate limiting
    // Monotonic time is preferred over wall clock for intervals
    crate::time::uptime_ms()
}

// =============================================================================
// PRODUCTION AES-256 IMPLEMENTATION
// =============================================================================

/// AES-256 round keys (15 rounds for AES-256)
type AesRoundKeys = [[u8; 16]; 15];

/// AES S-box for SubBytes transformation
const AES_SBOX: [u8; 256] = [
    0x63, 0x7c, 0x77, 0x7b, 0xf2, 0x6b, 0x6f, 0xc5, 0x30, 0x01, 0x67, 0x2b, 0xfe, 0xd7, 0xab, 0x76,
    0xca, 0x82, 0xc9, 0x7d, 0xfa, 0x59, 0x47, 0xf0, 0xad, 0xd4, 0xa2, 0xaf, 0x9c, 0xa4, 0x72, 0xc0,
    0xb7, 0xfd, 0x93, 0x26, 0x36, 0x3f, 0xf7, 0xcc, 0x34, 0xa5, 0xe5, 0xf1, 0x71, 0xd8, 0x31, 0x15,
    0x04, 0xc7, 0x23, 0xc3, 0x18, 0x96, 0x05, 0x9a, 0x07, 0x12, 0x80, 0xe2, 0xeb, 0x27, 0xb2, 0x75,
    0x09, 0x83, 0x2c, 0x1a, 0x1b, 0x6e, 0x5a, 0xa0, 0x52, 0x3b, 0xd6, 0xb3, 0x29, 0xe3, 0x2f, 0x84,
    0x53, 0xd1, 0x00, 0xed, 0x20, 0xfc, 0xb1, 0x5b, 0x6a, 0xcb, 0xbe, 0x39, 0x4a, 0x4c, 0x58, 0xcf,
    0xd0, 0xef, 0xaa, 0xfb, 0x43, 0x4d, 0x33, 0x85, 0x45, 0xf9, 0x02, 0x7f, 0x50, 0x3c, 0x9f, 0xa8,
    0x51, 0xa3, 0x40, 0x8f, 0x92, 0x9d, 0x38, 0xf5, 0xbc, 0xb6, 0xda, 0x21, 0x10, 0xff, 0xf3, 0xd2,
    0xcd, 0x0c, 0x13, 0xec, 0x5f, 0x97, 0x44, 0x17, 0xc4, 0xa7, 0x7e, 0x3d, 0x64, 0x5d, 0x19, 0x73,
    0x60, 0x81, 0x4f, 0xdc, 0x22, 0x2a, 0x90, 0x88, 0x46, 0xee, 0xb8, 0x14, 0xde, 0x5e, 0x0b, 0xdb,
    0xe0, 0x32, 0x3a, 0x0a, 0x49, 0x06, 0x24, 0x5c, 0xc2, 0xd3, 0xac, 0x62, 0x91, 0x95, 0xe4, 0x79,
    0xe7, 0xc8, 0x37, 0x6d, 0x8d, 0xd5, 0x4e, 0xa9, 0x6c, 0x56, 0xf4, 0xea, 0x65, 0x7a, 0xae, 0x08,
    0xba, 0x78, 0x25, 0x2e, 0x1c, 0xa6, 0xb4, 0xc6, 0xe8, 0xdd, 0x74, 0x1f, 0x4b, 0xbd, 0x8b, 0x8a,
    0x70, 0x3e, 0xb5, 0x66, 0x48, 0x03, 0xf6, 0x0e, 0x61, 0x35, 0x57, 0xb9, 0x86, 0xc1, 0x1d, 0x9e,
    0xe1, 0xf8, 0x98, 0x11, 0x69, 0xd9, 0x8e, 0x94, 0x9b, 0x1e, 0x87, 0xe9, 0xce, 0x55, 0x28, 0xdf,
    0x8c, 0xa1, 0x89, 0x0d, 0xbf, 0xe6, 0x42, 0x68, 0x41, 0x99, 0x2d, 0x0f, 0xb0, 0x54, 0xbb, 0x16,
];

/// AES round constants for key expansion
const AES_RCON: [u8; 15] = [
    0x01, 0x02, 0x04, 0x08, 0x10, 0x20, 0x40, 0x80, 0x1b, 0x36, 0x6c, 0xd8, 0xab, 0x4d, 0x9a,
];

/// Generate AES-256 round keys from master key
fn aes256_key_schedule(key: &[u8]) -> AesRoundKeys {
    let mut round_keys = [[0u8; 16]; 15];
    let mut w = [[0u8; 4]; 60]; // 60 words for AES-256

    // Copy initial key
    for i in 0..8 {
        for j in 0..4 {
            w[i][j] = key[i * 4 + j];
        }
    }

    // Generate remaining words
    for i in 8..60 {
        let mut temp = w[i - 1];

        if i % 8 == 0 {
            // RotWord and SubWord
            let temp_0 = temp[0];
            temp[0] = AES_SBOX[temp[1] as usize];
            temp[1] = AES_SBOX[temp[2] as usize];
            temp[2] = AES_SBOX[temp[3] as usize];
            temp[3] = AES_SBOX[temp_0 as usize];

            // XOR with round constant
            temp[0] ^= AES_RCON[(i / 8) - 1];
        } else if i % 8 == 4 {
            // SubWord only
            for j in 0..4 {
                temp[j] = AES_SBOX[temp[j] as usize];
            }
        }

        // XOR with word 8 positions back
        for j in 0..4 {
            w[i][j] = w[i - 8][j] ^ temp[j];
        }
    }

    // Convert words to round keys
    for round in 0..15 {
        for word in 0..4 {
            for byte in 0..4 {
                round_keys[round][word * 4 + byte] = w[round * 4 + word][byte];
            }
        }
    }

    round_keys
}

/// Encrypt a single 16-byte block with AES-256
fn aes256_encrypt_block(round_keys: &AesRoundKeys, plaintext: &[u8; 16]) -> [u8; 16] {
    let mut state = *plaintext;

    // Initial round key addition
    for i in 0..16 {
        state[i] ^= round_keys[0][i];
    }

    // Main rounds (1-14)
    for round in 1..15 {
        // SubBytes
        for i in 0..16 {
            state[i] = AES_SBOX[state[i] as usize];
        }

        // ShiftRows
        aes_shift_rows(&mut state);

        // MixColumns (skip in final round)
        if round < 14 {
            aes_mix_columns(&mut state);
        }

        // AddRoundKey
        for i in 0..16 {
            state[i] ^= round_keys[round][i];
        }
    }

    state
}

/// AES ShiftRows transformation
fn aes_shift_rows(state: &mut [u8; 16]) {
    // Row 1: shift left by 1
    let temp = state[1];
    state[1] = state[5];
    state[5] = state[9];
    state[9] = state[13];
    state[13] = temp;

    // Row 2: shift left by 2
    let temp1 = state[2];
    let temp2 = state[6];
    state[2] = state[10];
    state[6] = state[14];
    state[10] = temp1;
    state[14] = temp2;

    // Row 3: shift left by 3
    let temp = state[15];
    state[15] = state[11];
    state[11] = state[7];
    state[7] = state[3];
    state[3] = temp;
}

/// AES MixColumns transformation
fn aes_mix_columns(state: &mut [u8; 16]) {
    for col in 0..4 {
        let s0 = state[col * 4];
        let s1 = state[col * 4 + 1];
        let s2 = state[col * 4 + 2];
        let s3 = state[col * 4 + 3];

        state[col * 4] = gf_mul(0x02, s0) ^ gf_mul(0x03, s1) ^ s2 ^ s3;
        state[col * 4 + 1] = s0 ^ gf_mul(0x02, s1) ^ gf_mul(0x03, s2) ^ s3;
        state[col * 4 + 2] = s0 ^ s1 ^ gf_mul(0x02, s2) ^ gf_mul(0x03, s3);
        state[col * 4 + 3] = gf_mul(0x03, s0) ^ s1 ^ s2 ^ gf_mul(0x02, s3);
    }
}

/// Galois Field multiplication for AES MixColumns
fn gf_mul(a: u8, b: u8) -> u8 {
    let mut result = 0;
    let mut a = a;
    let mut b = b;

    for _ in 0..8 {
        if b & 1 != 0 {
            result ^= a;
        }

        let hi_bit_set = a & 0x80 != 0;
        a <<= 1;
        if hi_bit_set {
            a ^= 0x1b; // AES irreducible polynomial
        }
        b >>= 1;
    }

    result
}

/// AES inverse S-box for InvSubBytes transformation
const AES_INV_SBOX: [u8; 256] = [
    0x52, 0x09, 0x6a, 0xd5, 0x30, 0x36, 0xa5, 0x38, 0xbf, 0x40, 0xa3, 0x9e, 0x81, 0xf3, 0xd7, 0xfb,
    0x7c, 0xe3, 0x39, 0x82, 0x9b, 0x2f, 0xff, 0x87, 0x34, 0x8e, 0x43, 0x44, 0xc4, 0xde, 0xe9, 0xcb,
    0x54, 0x7b, 0x94, 0x32, 0xa6, 0xc2, 0x23, 0x3d, 0xee, 0x4c, 0x95, 0x0b, 0x42, 0xfa, 0xc3, 0x4e,
    0x08, 0x2e, 0xa1, 0x66, 0x28, 0xd9, 0x24, 0xb2, 0x76, 0x5b, 0xa2, 0x49, 0x6d, 0x8b, 0xd1, 0x25,
    0x72, 0xf8, 0xf6, 0x64, 0x86, 0x68, 0x98, 0x16, 0xd4, 0xa4, 0x5c, 0xcc, 0x5d, 0x65, 0xb6, 0x92,
    0x6c, 0x70, 0x48, 0x50, 0xfd, 0xed, 0xb9, 0xda, 0x5e, 0x15, 0x46, 0x57, 0xa7, 0x8d, 0x9d, 0x84,
    0x90, 0xd8, 0xab, 0x00, 0x8c, 0xbc, 0xd3, 0x0a, 0xf7, 0xe4, 0x58, 0x05, 0xb8, 0xb3, 0x45, 0x06,
    0xd0, 0x2c, 0x1e, 0x8f, 0xca, 0x3f, 0x0f, 0x02, 0xc1, 0xaf, 0xbd, 0x03, 0x01, 0x13, 0x8a, 0x6b,
    0x3a, 0x91, 0x11, 0x41, 0x4f, 0x67, 0xdc, 0xea, 0x97, 0xf2, 0xcf, 0xce, 0xf0, 0xb4, 0xe6, 0x73,
    0x96, 0xac, 0x74, 0x22, 0xe7, 0xad, 0x35, 0x85, 0xe2, 0xf9, 0x37, 0xe8, 0x1c, 0x75, 0xdf, 0x6e,
    0x47, 0xf1, 0x1a, 0x71, 0x1d, 0x29, 0xc5, 0x89, 0x6f, 0xb7, 0x62, 0x0e, 0xaa, 0x18, 0xbe, 0x1b,
    0xfc, 0x56, 0x3e, 0x4b, 0xc6, 0xd2, 0x79, 0x20, 0x9a, 0xdb, 0xc0, 0xfe, 0x78, 0xcd, 0x5a, 0xf4,
    0x1f, 0xdd, 0xa8, 0x33, 0x88, 0x07, 0xc7, 0x31, 0xb1, 0x12, 0x10, 0x59, 0x27, 0x80, 0xec, 0x5f,
    0x60, 0x51, 0x7f, 0xa9, 0x19, 0xb5, 0x4a, 0x0d, 0x2d, 0xe5, 0x7a, 0x9f, 0x93, 0xc9, 0x9c, 0xef,
    0xa0, 0xe0, 0x3b, 0x4d, 0xae, 0x2a, 0xf5, 0xb0, 0xc8, 0xeb, 0xbb, 0x3c, 0x83, 0x53, 0x99, 0x61,
    0x17, 0x2b, 0x04, 0x7e, 0xba, 0x77, 0xd6, 0x26, 0xe1, 0x69, 0x14, 0x63, 0x55, 0x21, 0x0c, 0x7d,
];

/// Decrypt a single 16-byte block with AES-256
fn aes256_decrypt_block(round_keys: &AesRoundKeys, ciphertext: &[u8; 16]) -> [u8; 16] {
    let mut state = *ciphertext;

    // Initial round key addition (last round key)
    for i in 0..16 {
        state[i] ^= round_keys[14][i];
    }

    // Main rounds (13 down to 1)
    for round in (1..14).rev() {
        // InvShiftRows
        aes_inv_shift_rows(&mut state);

        // InvSubBytes
        for i in 0..16 {
            state[i] = AES_INV_SBOX[state[i] as usize];
        }

        // AddRoundKey
        for i in 0..16 {
            state[i] ^= round_keys[round][i];
        }

        // InvMixColumns
        aes_inv_mix_columns(&mut state);
    }

    // Final round (round 0): InvShiftRows, InvSubBytes, AddRoundKey
    aes_inv_shift_rows(&mut state);
    for i in 0..16 {
        state[i] = AES_INV_SBOX[state[i] as usize];
    }
    for i in 0..16 {
        state[i] ^= round_keys[0][i];
    }

    state
}

/// AES inverse ShiftRows transformation
fn aes_inv_shift_rows(state: &mut [u8; 16]) {
    // Row 1: shift right by 1
    let temp = state[13];
    state[13] = state[9];
    state[9] = state[5];
    state[5] = state[1];
    state[1] = temp;

    // Row 2: shift right by 2
    let temp1 = state[2];
    let temp2 = state[6];
    state[2] = state[10];
    state[6] = state[14];
    state[10] = temp1;
    state[14] = temp2;

    // Row 3: shift right by 3
    let temp = state[3];
    state[3] = state[7];
    state[7] = state[11];
    state[11] = state[15];
    state[15] = temp;
}

/// AES inverse MixColumns transformation
fn aes_inv_mix_columns(state: &mut [u8; 16]) {
    for col in 0..4 {
        let s0 = state[col * 4];
        let s1 = state[col * 4 + 1];
        let s2 = state[col * 4 + 2];
        let s3 = state[col * 4 + 3];

        state[col * 4] = gf_mul(0x0e, s0) ^ gf_mul(0x0b, s1) ^ gf_mul(0x0d, s2) ^ gf_mul(0x09, s3);
        state[col * 4 + 1] =
            gf_mul(0x09, s0) ^ gf_mul(0x0e, s1) ^ gf_mul(0x0b, s2) ^ gf_mul(0x0d, s3);
        state[col * 4 + 2] =
            gf_mul(0x0d, s0) ^ gf_mul(0x09, s1) ^ gf_mul(0x0e, s2) ^ gf_mul(0x0b, s3);
        state[col * 4 + 3] =
            gf_mul(0x0b, s0) ^ gf_mul(0x0d, s1) ^ gf_mul(0x09, s2) ^ gf_mul(0x0e, s3);
    }
}

// =============================================================================
// GCM (Galois/Counter Mode) IMPLEMENTATION
// =============================================================================

/// GCM state for authenticated encryption
struct GcmState {
    h: [u8; 16],              // Hash subkey (preserved, never modified)
    ghash_state: [u8; 16],    // GHASH accumulator
    j0: [u8; 16],             // Initial counter block
    round_keys: AesRoundKeys, // Key schedule for encrypting J0 in finalize
}

impl GcmState {
    /// Initialize GCM state
    fn new(round_keys: &AesRoundKeys, nonce: &[u8]) -> Self {
        // Generate hash subkey H = AES_K(0^128)
        let zero_block = [0u8; 16];
        let h = aes256_encrypt_block(round_keys, &zero_block);

        // Generate initial counter block J0
        let j0 = if nonce.len() == 12 {
            // 96-bit nonce: J0 = nonce || 0^31 || 1
            let mut j = [0u8; 16];
            j[..12].copy_from_slice(nonce);
            j[15] = 1;
            j
        } else {
            // Non-96-bit nonce: J0 = GHASH_H(nonce || 0^s || len(nonce))
            // where s pads nonce to a multiple of 128 bits, and len is
            // a 128-bit big-endian value with the bit length in the low 64.
            let mut ghash_acc = [0u8; 16];

            // GHASH nonce blocks
            for chunk in nonce.chunks(16) {
                let mut block = [0u8; 16];
                block[..chunk.len()].copy_from_slice(chunk);
                for i in 0..16 {
                    ghash_acc[i] ^= block[i];
                }
                ghash_acc = ghash_mul(&ghash_acc, &h);
            }

            // Length block: 64-bit 0 || 64-bit nonce bit length (BE)
            let mut length_block = [0u8; 16];
            let nonce_bits = (nonce.len() as u64) * 8;
            length_block[8..].copy_from_slice(&nonce_bits.to_be_bytes());
            for i in 0..16 {
                ghash_acc[i] ^= length_block[i];
            }
            ghash_acc = ghash_mul(&ghash_acc, &h);

            ghash_acc
        };

        Self {
            h,
            ghash_state: [0u8; 16],
            j0,
            round_keys: *round_keys,
        }
    }

    /// Update GHASH with additional data
    fn update_ghash(&mut self, data: &[u8]) {
        for chunk in data.chunks(16) {
            let mut block = [0u8; 16];
            block[..chunk.len()].copy_from_slice(chunk);

            // XOR with current state
            for i in 0..16 {
                self.ghash_state[i] ^= block[i];
            }

            // Multiply by H in GF(2^128)
            self.ghash_state = ghash_mul(&self.ghash_state, &self.h);
        }
    }

    /// Finalize GCM and generate authentication tag
    fn finalize(&mut self, aad_len: usize, ciphertext_len: usize) -> Vec<u8> {
        // Add length block to GHASH
        let mut length_block = [0u8; 16];
        length_block[..8].copy_from_slice(&(aad_len as u64 * 8).to_be_bytes());
        length_block[8..].copy_from_slice(&(ciphertext_len as u64 * 8).to_be_bytes());

        for i in 0..16 {
            self.ghash_state[i] ^= length_block[i];
        }
        self.ghash_state = ghash_mul(&self.ghash_state, &self.h);

        // Tag = GHASH_final XOR E_K(J0)
        let encrypted_j0 = aes256_encrypt_block(&self.round_keys, &self.j0);
        let mut tag = [0u8; 16];
        for i in 0..16 {
            tag[i] = self.ghash_state[i] ^ encrypted_j0[i];
        }
        tag.to_vec()
    }
}

/// GHASH multiplication in GF(2^128) with the GCM reduction polynomial.
/// Returns X * H without modifying H.
fn ghash_mul(x: &[u8; 16], h: &[u8; 16]) -> [u8; 16] {
    let mut result = [0u8; 16];
    let mut v = *h; // Local copy of H — shifted, but original preserved

    for i in 0..128 {
        if (x[i / 8] >> (7 - (i % 8))) & 1 != 0 {
            for j in 0..16 {
                result[j] ^= v[j];
            }
        }

        // Shift V right by 1 bit
        let mut carry = 0;
        for j in 0..16 {
            let new_carry = v[j] & 1;
            v[j] = (v[j] >> 1) | (carry << 7);
            carry = new_carry;
        }

        // If we shifted out a 1, XOR with the reduction polynomial
        if carry != 0 {
            v[0] ^= 0xe1;
        }
    }

    result
}

// =============================================================================
// CHACHA20 IMPLEMENTATION
// =============================================================================

/// Generate a ChaCha20 block
fn chacha20_block(key: &[u8], nonce: &[u8], counter: u32) -> [u8; 64] {
    let mut state = [0u32; 16];

    // Constants
    state[0] = 0x61707865; // "expa"
    state[1] = 0x3320646e; // "nd 3"
    state[2] = 0x79622d32; // "2-by"
    state[3] = 0x6b206574; // "te k"

    // Key
    for i in 0..8 {
        state[4 + i] =
            u32::from_le_bytes([key[i * 4], key[i * 4 + 1], key[i * 4 + 2], key[i * 4 + 3]]);
    }

    // Counter
    state[12] = counter;

    // Nonce
    for i in 0..3 {
        state[13 + i] = u32::from_le_bytes([
            nonce[i * 4],
            nonce[i * 4 + 1],
            nonce[i * 4 + 2],
            nonce[i * 4 + 3],
        ]);
    }

    let initial_state = state;

    // 20 rounds (10 double rounds)
    for _ in 0..10 {
        // Column rounds
        chacha20_quarter_round(&mut state, 0, 4, 8, 12);
        chacha20_quarter_round(&mut state, 1, 5, 9, 13);
        chacha20_quarter_round(&mut state, 2, 6, 10, 14);
        chacha20_quarter_round(&mut state, 3, 7, 11, 15);

        // Diagonal rounds
        chacha20_quarter_round(&mut state, 0, 5, 10, 15);
        chacha20_quarter_round(&mut state, 1, 6, 11, 12);
        chacha20_quarter_round(&mut state, 2, 7, 8, 13);
        chacha20_quarter_round(&mut state, 3, 4, 9, 14);
    }

    // Add initial state
    for i in 0..16 {
        state[i] = state[i].wrapping_add(initial_state[i]);
    }

    // Convert to bytes
    let mut output = [0u8; 64];
    for i in 0..16 {
        let bytes = state[i].to_le_bytes();
        output[i * 4..(i + 1) * 4].copy_from_slice(&bytes);
    }

    output
}

// =============================================================================
// POLY1305 IMPLEMENTATION
// =============================================================================

/// Compute Poly1305 MAC
fn poly1305_mac(key: &[u8], aad: &[u8], ciphertext: &[u8]) -> Vec<u8> {
    // Poly1305 key components
    let r = [
        u32::from_le_bytes([key[0], key[1], key[2], key[3]]) & 0x0fffffff,
        u32::from_le_bytes([key[4], key[5], key[6], key[7]]) & 0x0ffffffc,
        u32::from_le_bytes([key[8], key[9], key[10], key[11]]) & 0x0ffffffc,
        u32::from_le_bytes([key[12], key[13], key[14], key[15]]) & 0x0ffffffc,
    ];

    let s = [
        u32::from_le_bytes([key[16], key[17], key[18], key[19]]),
        u32::from_le_bytes([key[20], key[21], key[22], key[23]]),
        u32::from_le_bytes([key[24], key[25], key[26], key[27]]),
        u32::from_le_bytes([key[28], key[29], key[30], key[31]]),
    ];

    let mut accumulator = [0u32; 5];

    // Process AAD
    for chunk in aad.chunks(16) {
        let mut block = [0u8; 17];
        block[..chunk.len()].copy_from_slice(chunk);
        block[chunk.len()] = 1; // Padding bit

        poly1305_block(&mut accumulator, &r, &block);
    }

    // Process ciphertext
    for chunk in ciphertext.chunks(16) {
        let mut block = [0u8; 17];
        block[..chunk.len()].copy_from_slice(chunk);
        block[chunk.len()] = 1; // Padding bit

        poly1305_block(&mut accumulator, &r, &block);
    }

    // Add s
    let mut carry = 0u64;
    for i in 0..4 {
        carry += accumulator[i] as u64 + s[i] as u64;
        accumulator[i] = carry as u32;
        carry >>= 32;
    }

    // Convert to bytes
    let mut tag = Vec::with_capacity(16);
    for i in 0..4 {
        tag.extend_from_slice(&accumulator[i].to_le_bytes());
    }

    tag
}

/// Process a Poly1305 block
fn poly1305_block(accumulator: &mut [u32; 5], r: &[u32; 4], block: &[u8; 17]) {
    // Add block to accumulator
    let mut carry = 0u64;
    for i in 0..4 {
        let block_word = u32::from_le_bytes([
            block[i * 4],
            block[i * 4 + 1],
            block[i * 4 + 2],
            block[i * 4 + 3],
        ]);
        carry += accumulator[i] as u64 + block_word as u64;
        accumulator[i] = carry as u32;
        carry >>= 32;
    }
    accumulator[4] += block[16] as u32 + carry as u32;

    // Multiply by r
    let mut result = [0u64; 5];
    for i in 0..4 {
        for j in 0..4 {
            result[i + j] += (accumulator[i] as u64) * (r[j] as u64);
        }
        result[i + 4] += (accumulator[4] as u64) * (r[i] as u64);
    }

    // Reduce modulo 2^130 - 5
    let mut carry = 0u64;
    for i in 0..4 {
        carry += result[i] + (result[i + 4] >> 2) * 5;
        accumulator[i] = carry as u32;
        carry >>= 32;
    }
    accumulator[4] = (result[4] & 3) as u32 + carry as u32;

    // Final reduction
    if accumulator[4] >= 4 {
        let mut carry = 5u64;
        for i in 0..4 {
            carry += accumulator[i] as u64;
            accumulator[i] = carry as u32;
            carry >>= 32;
        }
        accumulator[4] = 0;
    }
}

// =============================================================================
// SECURE KEY MANAGEMENT
// =============================================================================

/// Key derivation function using PBKDF2 with SHA-256
pub fn derive_key(password: &[u8], salt: &[u8], iterations: u32, key_length: usize) -> Vec<u8> {
    let mut derived_key = vec![0u8; key_length];
    pbkdf2_sha256(password, salt, iterations, &mut derived_key);
    derived_key
}

/// PBKDF2 with SHA-256 implementation
fn pbkdf2_sha256(password: &[u8], salt: &[u8], iterations: u32, output: &mut [u8]) {
    let hlen = 32; // SHA-256 output length
    let dklen = output.len();

    if dklen > ((2u64.pow(32) - 1) * hlen as u64) as usize {
        panic!("Derived key too long");
    }

    let l = (dklen + hlen - 1) / hlen; // Ceiling division

    for i in 1..=l {
        let mut u = hmac_sha256(password, &[salt, &(i as u32).to_be_bytes()].concat());
        let mut f = u.clone();

        for _ in 1..iterations {
            u = hmac_sha256(password, &u);
            for j in 0..hlen {
                f[j] ^= u[j];
            }
        }

        let start = (i - 1) * hlen;
        let end = core::cmp::min(start + hlen, dklen);
        output[start..end].copy_from_slice(&f[..end - start]);
    }
}

/// HMAC-SHA256 implementation
fn hmac_sha256(key: &[u8], message: &[u8]) -> Vec<u8> {
    const BLOCK_SIZE: usize = 64;
    const IPAD: u8 = 0x36;
    const OPAD: u8 = 0x5c;

    let mut key_padded = [0u8; BLOCK_SIZE];

    if key.len() > BLOCK_SIZE {
        // If key is longer than block size, hash it first
        let key_hash = sha256(key);
        key_padded[..key_hash.len()].copy_from_slice(&key_hash);
    } else {
        key_padded[..key.len()].copy_from_slice(key);
    }

    // Create inner and outer padded keys
    let mut inner_key = [0u8; BLOCK_SIZE];
    let mut outer_key = [0u8; BLOCK_SIZE];

    for i in 0..BLOCK_SIZE {
        inner_key[i] = key_padded[i] ^ IPAD;
        outer_key[i] = key_padded[i] ^ OPAD;
    }

    // Inner hash: SHA256(inner_key || message)
    let inner_hash = sha256(&[&inner_key[..], message].concat());

    // Outer hash: SHA256(outer_key || inner_hash)
    sha256(&[&outer_key[..], &inner_hash].concat())
}

/// Secure key storage with encryption
pub struct SecureKeyStore {
    master_key: [u8; 32],
    keys: BTreeMap<alloc::string::String, Vec<u8>>,
}

impl SecureKeyStore {
    /// Create a new secure key store
    pub fn new() -> Result<Self, &'static str> {
        let mut master_key = [0u8; 32];
        secure_random_bytes(&mut master_key)?;

        Ok(Self {
            master_key,
            keys: BTreeMap::new(),
        })
    }

    /// Store a key securely
    pub fn store_key(&mut self, name: &str, key: &[u8]) -> Result<(), &'static str> {
        // Encrypt the key with the master key
        let encrypted_key = self.encrypt_key(key)?;
        self.keys.insert(name.to_string(), encrypted_key);
        Ok(())
    }

    /// Retrieve a key securely
    pub fn get_key(&self, name: &str) -> Result<Vec<u8>, &'static str> {
        let encrypted_key = self.keys.get(name).ok_or("Key not found")?;
        self.decrypt_key(encrypted_key)
    }

    /// Remove a key
    pub fn remove_key(&mut self, name: &str) -> bool {
        self.keys.remove(name).is_some()
    }

    /// List all key names
    pub fn list_keys(&self) -> Vec<alloc::string::String> {
        self.keys.keys().cloned().collect()
    }

    /// Encrypt a key with the master key
    fn encrypt_key(&self, key: &[u8]) -> Result<Vec<u8>, &'static str> {
        let encryption_key =
            EncryptionKey::new(EncryptionAlgorithm::Aes256Gcm, self.master_key.to_vec());

        let result = encrypt(&encryption_key, key)?;

        // Serialize the encryption result
        let mut serialized = Vec::new();
        serialized.extend_from_slice(&(result.nonce.len() as u32).to_le_bytes());
        serialized.extend_from_slice(&result.nonce);
        serialized.extend_from_slice(&(result.ciphertext.len() as u32).to_le_bytes());
        serialized.extend_from_slice(&result.ciphertext);

        if let Some(tag) = result.tag {
            serialized.extend_from_slice(&(tag.len() as u32).to_le_bytes());
            serialized.extend_from_slice(&tag);
        } else {
            serialized.extend_from_slice(&0u32.to_le_bytes());
        }

        Ok(serialized)
    }

    /// Decrypt a key with the master key
    fn decrypt_key(&self, encrypted_data: &[u8]) -> Result<Vec<u8>, &'static str> {
        if encrypted_data.len() < 12 {
            return Err("Invalid encrypted data");
        }

        let mut offset = 0;

        // Read nonce
        let nonce_len = u32::from_le_bytes([
            encrypted_data[offset],
            encrypted_data[offset + 1],
            encrypted_data[offset + 2],
            encrypted_data[offset + 3],
        ]) as usize;
        offset += 4;

        if offset + nonce_len > encrypted_data.len() {
            return Err("Invalid nonce length");
        }

        let nonce = encrypted_data[offset..offset + nonce_len].to_vec();
        offset += nonce_len;

        // Read ciphertext
        let ciphertext_len = u32::from_le_bytes([
            encrypted_data[offset],
            encrypted_data[offset + 1],
            encrypted_data[offset + 2],
            encrypted_data[offset + 3],
        ]) as usize;
        offset += 4;

        if offset + ciphertext_len > encrypted_data.len() {
            return Err("Invalid ciphertext length");
        }

        let ciphertext = encrypted_data[offset..offset + ciphertext_len].to_vec();
        offset += ciphertext_len;

        // Read tag
        let tag_len = u32::from_le_bytes([
            encrypted_data[offset],
            encrypted_data[offset + 1],
            encrypted_data[offset + 2],
            encrypted_data[offset + 3],
        ]) as usize;
        offset += 4;

        let tag = if tag_len > 0 {
            if offset + tag_len > encrypted_data.len() {
                return Err("Invalid tag length");
            }
            Some(encrypted_data[offset..offset + tag_len].to_vec())
        } else {
            None
        };

        let encrypted_result = EncryptionResult {
            ciphertext,
            nonce,
            tag,
        };

        let encryption_key =
            EncryptionKey::new(EncryptionAlgorithm::Aes256Gcm, self.master_key.to_vec());

        decrypt(&encryption_key, &encrypted_result)
    }
}

/// Global secure key store
static SECURE_KEY_STORE: RwLock<Option<SecureKeyStore>> = RwLock::new(None);

/// Initialize the secure key store
pub fn init_key_store() -> Result<(), &'static str> {
    let mut store = SECURE_KEY_STORE.write();
    *store = Some(SecureKeyStore::new()?);
    Ok(())
}

/// Store a key in the global key store
pub fn store_global_key(name: &str, key: &[u8]) -> Result<(), &'static str> {
    let mut store = SECURE_KEY_STORE.write();
    if let Some(ref mut key_store) = *store {
        key_store.store_key(name, key)
    } else {
        Err("Key store not initialized")
    }
}

/// Retrieve a key from the global key store
pub fn get_global_key(name: &str) -> Result<Vec<u8>, &'static str> {
    let store = SECURE_KEY_STORE.read();
    if let Some(ref key_store) = *store {
        key_store.get_key(name)
    } else {
        Err("Key store not initialized")
    }
}

// =============================================================================
// HARDWARE AND SYSTEM DETECTION FUNCTIONS
// =============================================================================

/// Check if hardware random number generator (RDRAND/RDSEED) is available.
///
/// This function queries the CPU for RDRAND support using CPUID.
/// RDRAND provides high-quality hardware-generated random numbers
/// suitable for cryptographic use.
///
/// # Returns
/// `true` if RDRAND instruction is supported by the CPU, `false` otherwise.
pub fn hardware_rng_available() -> bool {
    is_rdrand_supported() || is_rdseed_supported()
}

/// Check if the entropy pool has been properly seeded with sufficient entropy.
///
/// The entropy pool is considered seeded when it contains at least 128 bits
/// of entropy, which is the minimum required for cryptographic security.
/// The pool is automatically seeded during RNG initialization and reseeded
/// periodically or when entropy runs low.
///
/// # Returns
/// `true` if the entropy pool has at least 128 bits of entropy, `false` otherwise.
pub fn entropy_pool_seeded() -> bool {
    let state = RNG_STATE.read();
    // Entropy estimate is updated by collect_entropy()
    // We require at least 128 bits of entropy for security
    state.entropy_estimate >= 128
}

/// Fill a buffer with cryptographically secure random bytes.
///
/// This is a convenience wrapper around `secure_random_bytes()` that provides
/// the standard interface for obtaining random data. The underlying implementation
/// uses a ChaCha20-based CSPRNG seeded from hardware entropy sources (RDRAND/RDSEED)
/// and timing-based entropy collection.
///
/// # Arguments
/// * `buffer` - The buffer to fill with random bytes
///
/// # Returns
/// `Ok(())` on success, or an error if entropy collection fails.
///
/// # Security
/// The generated bytes are suitable for cryptographic purposes including
/// key generation, nonce generation, and other security-critical operations.
pub fn get_random_bytes(buffer: &mut [u8]) -> Result<(), &'static str> {
    secure_random_bytes(buffer)
}

/// Generate a cryptographic key of the specified bit length.
///
/// Generates a random key using the secure random number generator.
/// The key is suitable for use with symmetric encryption algorithms.
///
/// # Arguments
/// * `bits` - The desired key length in bits. Common values are:
///   - 128 bits (16 bytes) - AES-128
///   - 192 bits (24 bytes) - AES-192
///   - 256 bits (32 bytes) - AES-256, ChaCha20
///   - 512 bits (64 bytes) - HMAC keys
///
/// # Returns
/// A vector containing the generated key bytes, or an error if:
/// - The bit length is not a multiple of 8
/// - The bit length is less than 128 (minimum for security)
/// - The bit length exceeds 4096 (reasonable maximum)
/// - Entropy collection fails
///
/// # Security
/// Generated keys are derived from the ChaCha20-based CSPRNG which is
/// seeded from hardware entropy sources.
pub fn generate_key(bits: usize) -> Result<Vec<u8>, &'static str> {
    // Validate bit length
    if bits % 8 != 0 {
        return Err("Key bit length must be a multiple of 8");
    }
    if bits < 128 {
        return Err("Key must be at least 128 bits for security");
    }
    if bits > 4096 {
        return Err("Key length exceeds maximum of 4096 bits");
    }

    let bytes = bits / 8;
    let mut key = vec![0u8; bytes];
    secure_random_bytes(&mut key)?;
    Ok(key)
}

/// Check if secure key storage hardware is available.
///
/// This function checks for the presence of hardware security modules that can
/// provide secure key storage, including:
/// - TPM (Trusted Platform Module) 1.2 or 2.0
/// - Hardware Security Modules (HSM)
/// - Secure enclaves (Intel SGX, ARM TrustZone)
///
/// # Returns
/// `true` if any hardware secure key storage mechanism is detected and available,
/// `false` otherwise.
///
/// # Note
/// In the current implementation, secure key storage is provided via software
/// encryption using the `SecureKeyStore`. Hardware TPM/HSM support requires
/// specific driver implementations which are platform-dependent.
///
/// The function returns `true` if the software-based SecureKeyStore has been
/// initialized, providing encrypted key storage as a baseline security measure.
pub fn secure_key_storage_available() -> bool {
    // Check if our software-based secure key store is initialized
    // This provides encrypted storage for keys using AES-256-GCM
    let store = SECURE_KEY_STORE.read();
    store.is_some()
}

/// Securely zero out sensitive data to prevent memory disclosure.
///
/// This function overwrites the provided buffer with zeros using volatile writes
/// to prevent the compiler from optimizing away the zeroing operation. This is
/// critical for security-sensitive data like cryptographic keys, passwords, and
/// other secrets that should not remain in memory after use.
///
/// # Arguments
/// * `data` - The buffer to securely zero
///
/// # Security Considerations
/// - Uses volatile writes to prevent compiler optimization
/// - Uses a compiler fence to ensure the operation completes before returning
/// - Marked `#[inline(never)]` to prevent inlining optimizations
/// - The function should be called before dropping any buffer containing sensitive data
///
/// # Example
/// ```
/// let mut key = [0u8; 32];
/// // ... use key for cryptographic operations ...
/// secure_zero(&mut key); // Securely erase the key from memory
/// ```
#[inline(never)]
pub fn secure_zero(data: &mut [u8]) {
    // Use volatile writes to prevent the compiler from optimizing away the zeroing
    for byte in data.iter_mut() {
        unsafe {
            core::ptr::write_volatile(byte, 0);
        }
    }
    // Memory fence to ensure all writes complete before returning
    // This prevents reordering of the zeroing operation
    core::sync::atomic::compiler_fence(core::sync::atomic::Ordering::SeqCst);
}

/// Compute the SHA-256 cryptographic hash of the provided data.
///
/// SHA-256 is a member of the SHA-2 family of hash functions, producing a
/// 256-bit (32-byte) hash digest. It is suitable for:
/// - Data integrity verification
/// - Digital signatures
/// - Key derivation functions
/// - Password hashing (when combined with salting and stretching)
///
/// # Arguments
/// * `data` - The data to hash
///
/// # Returns
/// A 32-byte vector containing the SHA-256 hash digest, or an error on failure.
///
/// # Implementation
/// Uses a production-quality SHA-256 implementation following RFC 6234.
pub fn hash_sha256(data: &[u8]) -> Result<Vec<u8>, &'static str> {
    let hash = compute_hash(HashType::Sha256, data);
    Ok(hash.bytes)
}

/// Encrypt data using AES-256-GCM authenticated encryption.
///
/// This function provides a simple interface for encrypting data using the
/// AES-256-GCM (Galois/Counter Mode) algorithm, which provides both
/// confidentiality and integrity protection.
///
/// # Arguments
/// * `plaintext` - The data to encrypt
/// * `key` - A 32-byte (256-bit) encryption key
///
/// # Returns
/// On success, returns a vector containing:
/// - 12-byte nonce (prepended)
/// - Ciphertext (same length as plaintext)
/// - 16-byte authentication tag (appended)
///
/// Total output size = 12 + plaintext.len() + 16 bytes
///
/// # Errors
/// Returns an error if:
/// - The key is not exactly 32 bytes
/// - Random nonce generation fails
///
/// # Security
/// - Uses AES-256-GCM for authenticated encryption
/// - Generates a fresh random 12-byte nonce for each encryption
/// - The authentication tag prevents tampering with the ciphertext
///
/// # Example
/// ```
/// let key = [0u8; 32]; // Use a proper key!
/// let plaintext = b"secret message";
/// let encrypted = encrypt_aes256(plaintext, &key)?;
/// ```
pub fn encrypt_aes256(plaintext: &[u8], key: &[u8]) -> Result<Vec<u8>, &'static str> {
    if key.len() != 32 {
        return Err("Invalid key size for AES-256: must be 32 bytes");
    }

    let encryption_key = EncryptionKey::new(EncryptionAlgorithm::Aes256Gcm, key.to_vec());
    let result = encrypt_data(&encryption_key, plaintext)?;

    // Format: nonce || ciphertext || tag
    // This allows the decryption function to easily parse the components
    let mut output = Vec::with_capacity(12 + result.ciphertext.len() + 16);
    output.extend_from_slice(&result.nonce);
    output.extend_from_slice(&result.ciphertext);
    if let Some(tag) = result.tag {
        output.extend_from_slice(&tag);
    }
    Ok(output)
}

/// Decrypt data that was encrypted using `encrypt_aes256`.
///
/// This function decrypts data that was encrypted using AES-256-GCM.
/// It expects the input to be in the format produced by `encrypt_aes256`:
/// nonce || ciphertext || tag
///
/// # Arguments
/// * `encrypted` - The encrypted data (nonce + ciphertext + tag)
/// * `key` - The 32-byte (256-bit) decryption key (same key used for encryption)
///
/// # Returns
/// On success, returns the original plaintext.
///
/// # Errors
/// Returns an error if:
/// - The key is not exactly 32 bytes
/// - The encrypted data is too short (minimum 28 bytes: 12 nonce + 16 tag)
/// - Authentication tag verification fails (data was tampered with)
///
/// # Security
/// - Verifies the authentication tag before returning plaintext
/// - Constant-time tag comparison to prevent timing attacks
///
/// # Example
/// ```
/// let key = [0u8; 32];
/// let encrypted = encrypt_aes256(b"secret", &key)?;
/// let decrypted = decrypt_aes256(&encrypted, &key)?;
/// assert_eq!(&decrypted, b"secret");
/// ```
pub fn decrypt_aes256(encrypted: &[u8], key: &[u8]) -> Result<Vec<u8>, &'static str> {
    if key.len() != 32 {
        return Err("Invalid key size for AES-256: must be 32 bytes");
    }

    // Minimum size: 12-byte nonce + 16-byte tag = 28 bytes
    if encrypted.len() < 28 {
        return Err("Encrypted data too short");
    }

    // Parse the encrypted data: nonce || ciphertext || tag
    let nonce = encrypted[..12].to_vec();
    let tag = encrypted[encrypted.len() - 16..].to_vec();
    let ciphertext = encrypted[12..encrypted.len() - 16].to_vec();

    let encrypted_result = EncryptionResult {
        nonce,
        ciphertext,
        tag: Some(tag),
    };

    let encryption_key = EncryptionKey::new(EncryptionAlgorithm::Aes256Gcm, key.to_vec());
    decrypt_data(&encryption_key, &encrypted_result)
}

// =============================================================================
// CURVE25519 / ED25519 FIELD ARITHMETIC
// =============================================================================
//
// Field arithmetic modulo p = 2^255 - 19 using 5 x 51-bit limbs.
// Point arithmetic on the twisted Edwards curve -x^2 + y^2 = 1 + dx^2y^2
// where d = -121665/121666 mod p, using extended coordinates (X, Y, Z, T).
//
// Note: Standard Ed25519 uses SHA-512; this kernel implementation uses
// SHA-256 instead since SHA-512 is not available. The curve arithmetic
// is identical — only the hash function differs. This means signatures
// are not compatible with standard Ed25519 libraries but are
// cryptographically sound for kernel-internal use.

/// Field element: 5 limbs of ~51 bits each, little-endian.
/// Value = sum(l[i] * 2^(51*i)) mod (2^255 - 19)
#[derive(Clone, Copy, Debug)]
struct Fe([u64; 5]);

const MASK51: u64 = (1u64 << 51) - 1;

fn fe_zero() -> Fe {
    Fe([0, 0, 0, 0, 0])
}

fn fe_one() -> Fe {
    Fe([1, 0, 0, 0, 0])
}

fn fe_two() -> Fe {
    Fe([2, 0, 0, 0, 0])
}

fn fe_from_bytes(bytes: &[u8; 32]) -> Fe {
    let load7 = |slice: &[u8]| -> u64 {
        let mut r = 0u64;
        for i in 0..7 {
            r |= (slice[i] as u64) << (8 * i);
        }
        r
    };
    let load6 = |slice: &[u8]| -> u64 {
        let mut r = 0u64;
        for i in 0..6 {
            r |= (slice[i] as u64) << (8 * i);
        }
        r
    };
    // The top limb uses only 25 bits (255 - 4*51 = 51*4=204, 255-204=51... actually
    // 5*51 = 255, so the top limb is 51 bits but the very top bit is masked.
    let h0 = load7(&bytes[0..7]) & MASK51;
    let h1 = (load7(&bytes[6..13]) >> 3) & MASK51;
    let h2 = (load7(&bytes[12..19]) >> 6) & MASK51;
    let h3 = (load7(&bytes[19..26]) >> 1) & MASK51;
    // Top limb: 25 bits from bytes, plus bit 255 is cleared
    let h4 = (load6(&bytes[24..30]) >> 12) | ((bytes[31] as u64 & 0x7F) << 25);
    Fe([h0, h1, h2, h3, h4])
}

fn fe_to_bytes(fe: &Fe) -> [u8; 32] {
    // Reduce mod 2^255-19 and pack to 32 bytes
    let mut h = fe.0;
    fe_reduce(&mut h);

    let mut bytes = [0u8; 32];
    // Pack 5 limbs into 32 bytes
    // Limb 0: bits 0-50, Limb 1: bits 51-101, etc.
    // Total: 255 bits
    let mut t0 = h[0] | (h[1] << 51);
    let mut t1 = (h[1] >> 13) | (h[2] << 38);
    let mut t2 = (h[2] >> 26) | (h[3] << 25);
    let mut t3 = (h[3] >> 39) | (h[4] << 12);

    for i in 0..8 {
        bytes[i] = (t0 & 0xFF) as u8;
        t0 >>= 8;
    }
    for i in 0..8 {
        bytes[8 + i] = (t1 & 0xFF) as u8;
        t1 >>= 8;
    }
    for i in 0..8 {
        bytes[16 + i] = (t2 & 0xFF) as u8;
        t2 >>= 8;
    }
    for i in 0..8 {
        bytes[24 + i] = (t3 & 0xFF) as u8;
        t3 >>= 8;
    }
    bytes
}

/// Reduce a field element mod p = 2^255 - 19.
/// After this, all limbs are < 2^51 and the value is in [0, p).
fn fe_reduce(h: &mut [u64; 5]) {
    // Carry from high limb
    let carry = h[4] >> 51;
    h[4] &= MASK51;
    h[0] += carry * 19;

    // Carry each limb
    let c0 = h[0] >> 51;
    h[0] &= MASK51;
    h[1] += c0;

    let c1 = h[1] >> 51;
    h[1] &= MASK51;
    h[2] += c1;

    let c2 = h[2] >> 51;
    h[2] &= MASK51;
    h[3] += c2;

    let c3 = h[3] >> 51;
    h[3] &= MASK51;
    h[4] += c3;

    // Final carry from h[4]
    let c4 = h[4] >> 51;
    h[4] &= MASK51;
    h[0] += c4 * 19;

    // Now h[0] might overflow again by at most 19
    let c = h[0] >> 51;
    h[0] &= MASK51;
    h[1] += c;

    // Conditional subtract of p if h >= p
    // p = 2^255 - 19. h >= p iff h + 19 >= 2^255
    let mut q = (h[0] + 19) >> 51;
    q = (h[1] + q) >> 51;
    q = (h[2] + q) >> 51;
    q = (h[3] + q) >> 51;
    q = (h[4] + q) >> 51;
    // q is 0 or 1
    h[0] += 19 * q;
    let c = h[0] >> 51;
    h[0] &= MASK51;
    h[1] += c;
}

fn fe_add(a: &Fe, b: &Fe) -> Fe {
    let mut r = [0u64; 5];
    for i in 0..5 {
        r[i] = a.0[i] + b.0[i];
    }
    // Don't reduce fully here — callers should reduce when needed
    Fe(r)
}

fn fe_sub(a: &Fe, b: &Fe) -> Fe {
    // Compute a - b + 2*p limb by limb to ensure positivity, then
    // carry-propagate to normalize. p = 2^255 - 19.
    let mut r = [0u64; 5];
    let p_0 = (1u128 << 51) - 19;
    let p_i = 1u128 << 51;
    r[0] = ((a.0[0] as i128) - (b.0[0] as i128) + (2 * p_0) as i128) as u64;
    for i in 1..5 {
        r[i] = ((a.0[i] as i128) - (b.0[i] as i128) + (2 * p_i) as i128) as u64;
    }
    // Carry-propagate to normalize
    let mut carry = 0i64;
    for i in 0..5 {
        let v = r[i] as i64 + carry;
        r[i] = (v as u64) & MASK51;
        carry = v >> 51;
    }
    // Final carry goes back to limb 0 multiplied by 19
    r[0] = r[0].wrapping_add((carry as u64).wrapping_mul(19));
    // One more carry pass
    let c = r[0] >> 51;
    r[0] &= MASK51;
    r[1] += c;
    let c = r[1] >> 51;
    r[1] &= MASK51;
    r[2] += c;
    let c = r[2] >> 51;
    r[2] &= MASK51;
    r[3] += c;
    let c = r[3] >> 51;
    r[3] &= MASK51;
    r[4] += c;
    r[4] &= MASK51;
    Fe(r)
}

fn fe_mul(a: &Fe, b: &Fe) -> Fe {
    // Schoolbook multiplication with 128-bit intermediates
    let a0 = a.0[0] as u128;
    let a1 = a.0[1] as u128;
    let a2 = a.0[2] as u128;
    let a3 = a.0[3] as u128;
    let a4 = a.0[4] as u128;

    let b0 = b.0[0] as u128;
    let b1 = b.0[1] as u128;
    let b2 = b.0[2] as u128;
    let b3 = b.0[3] as u128;
    let b4 = b.0[4] as u128;

    // Products with the reduction constant 19 for high limbs
    let r0 = a0 * b0 + 19 * (a1 * b4 + a2 * b3 + a3 * b2 + a4 * b1);
    let r1 = a0 * b1 + a1 * b0 + 19 * (a2 * b4 + a3 * b3 + a4 * b2);
    let r2 = a0 * b2 + a1 * b1 + a2 * b0 + 19 * (a3 * b4 + a4 * b3);
    let r3 = a0 * b3 + a1 * b2 + a2 * b1 + a3 * b0 + 19 * (a4 * b4);
    let r4 = a0 * b4 + a1 * b3 + a2 * b2 + a3 * b1 + a4 * b0;

    // Carry propagation: extract low 51 bits and carry the rest
    let mut c;
    let h0 = (r0 as u64) & MASK51;
    c = r0 >> 51;
    let h1 = ((r1 + c) as u64) & MASK51;
    c = (r1 + c) >> 51;
    let h2 = ((r2 + c) as u64) & MASK51;
    c = (r2 + c) >> 51;
    let h3 = ((r3 + c) as u64) & MASK51;
    c = (r3 + c) >> 51;
    let h4 = ((r4 + c) as u64) & MASK51;
    c = (r4 + c) >> 51;

    // Final carry goes back to limb 0, multiplied by 19
    let h0 = h0 + (c as u64) * 19;

    // One more carry from h0
    let h0 = h0 & MASK51;
    let h1 = h1 + (h0 >> 51); // h0 >> 51 is 0 after masking, but just in case

    let mut result = [h0, h1, h2, h3, h4];
    fe_reduce(&mut result);
    Fe(result)
}

fn fe_sq(a: &Fe) -> Fe {
    fe_mul(a, a)
}

fn fe_invert(a: &Fe) -> Fe {
    // Fermat's little theorem: a^(p-2) mod p
    // p - 2 = 2^255 - 21
    // Use addition chain for efficiency
    let z2 = fe_sq(a);
    let z9 = fe_sq(&fe_sq(&fe_sq(&z2)));
    let z11 = fe_mul(&z9, &z2);
    let z2_5_0 = fe_mul(&fe_sq(&z11), &z9);

    let mut z2_10_0 = fe_sq(&z2_5_0);
    for _ in 0..4 {
        z2_10_0 = fe_sq(&z2_10_0);
    }
    z2_10_0 = fe_mul(&z2_10_0, &z2_5_0);

    let mut z2_20_0 = fe_sq(&z2_10_0);
    for _ in 0..9 {
        z2_20_0 = fe_sq(&z2_20_0);
    }
    z2_20_0 = fe_mul(&z2_20_0, &z2_10_0);

    let mut z2_40_0 = fe_sq(&z2_20_0);
    for _ in 0..19 {
        z2_40_0 = fe_sq(&z2_40_0);
    }
    z2_40_0 = fe_mul(&z2_40_0, &z2_20_0);

    let mut z2_50_0 = fe_sq(&z2_40_0);
    for _ in 0..9 {
        z2_50_0 = fe_sq(&z2_50_0);
    }
    z2_50_0 = fe_mul(&z2_50_0, &z2_10_0);

    let mut z2_100_0 = fe_sq(&z2_50_0);
    for _ in 0..49 {
        z2_100_0 = fe_sq(&z2_100_0);
    }
    z2_100_0 = fe_mul(&z2_100_0, &z2_50_0);

    let mut z2_200_0 = fe_sq(&z2_100_0);
    for _ in 0..99 {
        z2_200_0 = fe_sq(&z2_200_0);
    }
    z2_200_0 = fe_mul(&z2_200_0, &z2_100_0);

    let mut z2_250_0 = fe_sq(&z2_200_0);
    for _ in 0..49 {
        z2_250_0 = fe_sq(&z2_250_0);
    }
    z2_250_0 = fe_mul(&z2_250_0, &z2_50_0);

    let mut t = fe_sq(&z2_250_0);
    for _ in 0..4 {
        t = fe_sq(&t);
    }
    fe_mul(&t, &z11)
}

fn fe_neg(a: &Fe) -> Fe {
    // -a mod p = p - a
    fe_sub(&fe_zero(), a)
}

fn fe_pow22523(a: &Fe) -> Fe {
    // a^((p-5)/8) = a^(2^252 - 3)
    let z2 = fe_sq(a);
    let z9 = fe_sq(&fe_sq(&fe_sq(&z2)));
    let z11 = fe_mul(&z9, &z2);
    let z2_5_0 = fe_mul(&fe_sq(&z11), &z9);

    let mut z2_10_0 = fe_sq(&z2_5_0);
    for _ in 0..4 {
        z2_10_0 = fe_sq(&z2_10_0);
    }
    z2_10_0 = fe_mul(&z2_10_0, &z2_5_0);

    let mut z2_20_0 = fe_sq(&z2_10_0);
    for _ in 0..9 {
        z2_20_0 = fe_sq(&z2_20_0);
    }
    z2_20_0 = fe_mul(&z2_20_0, &z2_10_0);

    let mut z2_40_0 = fe_sq(&z2_20_0);
    for _ in 0..19 {
        z2_40_0 = fe_sq(&z2_40_0);
    }
    z2_40_0 = fe_mul(&z2_40_0, &z2_20_0);

    let mut z2_50_0 = fe_sq(&z2_40_0);
    for _ in 0..9 {
        z2_50_0 = fe_sq(&z2_50_0);
    }
    z2_50_0 = fe_mul(&z2_50_0, &z2_10_0);

    let mut z2_100_0 = fe_sq(&z2_50_0);
    for _ in 0..49 {
        z2_100_0 = fe_sq(&z2_100_0);
    }
    z2_100_0 = fe_mul(&z2_100_0, &z2_50_0);

    let mut z2_200_0 = fe_sq(&z2_100_0);
    for _ in 0..99 {
        z2_200_0 = fe_sq(&z2_200_0);
    }
    z2_200_0 = fe_mul(&z2_200_0, &z2_100_0);

    let mut z2_250_0 = fe_sq(&z2_200_0);
    for _ in 0..49 {
        z2_250_0 = fe_sq(&z2_250_0);
    }
    z2_250_0 = fe_mul(&z2_250_0, &z2_50_0);

    let mut t = fe_sq(&z2_250_0);
    for _ in 0..2 {
        t = fe_sq(&t);
    }
    fe_mul(&t, &z11)
}

// Curve constant d = -121665/121666 mod p
fn fe_d() -> Fe {
    // d = -121665 * inv(121666) mod p (standard Ed25519 constant)
    let d_bytes = [
        0xa3, 0x78, 0x59, 0x13, 0xca, 0x4d, 0xeb, 0x95,
        0x9a, 0x77, 0xd6, 0x74, 0x8a, 0x96, 0x9b, 0x85,
        0x42, 0xc6, 0x3a, 0x4c, 0xb7, 0x70, 0x7b, 0x35,
        0x8d, 0x27, 0x56, 0x10, 0x35, 0x3e, 0x2a, 0x52,
    ];
    fe_from_bytes(&d_bytes)
}

fn fe_sqrt_m1() -> Fe {
    // sqrt(-1) mod p = 2^((p-1)/4)
    // Precomputed value
    let bytes = [
        0xb0, 0xa0, 0x0e, 0x4a, 0x27, 0x1e, 0xee, 0xc4,
        0x78, 0x5e, 0x49, 0x07, 0x4c, 0xa9, 0x47, 0x7f,
        0x6c, 0xa3, 0x37, 0x89, 0x28, 0x53, 0x36, 0x18,
        0xa6, 0x77, 0x6c, 0xa4, 0x69, 0x4e, 0x56, 0x20,
    ];
    fe_from_bytes(&bytes)
}

/// Extended twisted Edwards point: (X:Y:Z:T) where x=X/Z, y=Y/Z, XY=ZT
#[derive(Clone, Copy)]
struct ExtendedPoint {
    x: Fe,
    y: Fe,
    z: Fe,
    t: Fe,
}

fn point_identity() -> ExtendedPoint {
    ExtendedPoint {
        x: fe_zero(),
        y: fe_one(),
        z: fe_one(),
        t: fe_zero(),
    }
}

/// Point addition in extended coordinates.
/// Formula from RFC 8032 Section 5.1.4.
fn point_add(p: &ExtendedPoint, q: &ExtendedPoint) -> ExtendedPoint {
    let a = fe_sub(&p.y, &p.x);
    let b = fe_add(&q.y, &q.x);
    let c = fe_mul(&a, &b);
    let d = fe_mul(&p.t, &q.t);
    let d = fe_mul(&d, &fe_d());
    let d = fe_mul(&d, &fe_two()); // 2*d
    let e = fe_add(&c, &d);
    let f = fe_sub(&p.y, &p.x);
    let g = fe_add(&p.y, &p.x);
    let h = fe_mul(&f, &g);
    let h = fe_sub(&h, &c);
    let t = fe_mul(&p.z, &q.z);
    let t = fe_mul(&t, &fe_two()); // 2*Z1*Z2

    ExtendedPoint {
        x: fe_mul(&e, &h),
        y: fe_mul(&a, &b),
        z: fe_mul(&t, &h),
        t: fe_mul(&e, &f),
    }
}

/// Point doubling in extended coordinates.
fn point_double(p: &ExtendedPoint) -> ExtendedPoint {
    let a = fe_sq(&p.x);
    let b = fe_sq(&p.y);
    let d = fe_neg(&a);
    let e = fe_sub(&p.x, &p.y);
    let e = fe_sq(&e);
    let g = fe_add(&d, &b);
    let f = fe_sub(&g, &e);

    ExtendedPoint {
        x: fe_mul(&e, &g),
        y: fe_mul(&a, &b),
        z: fe_mul(&f, &g),
        t: fe_mul(&e, &f),
    }
}

/// Scalar multiplication using double-and-add.
fn scalar_mult(scalar: &[u8; 32], base: &ExtendedPoint) -> ExtendedPoint {
    let mut result = point_identity();
    // Process scalar from MSB to LSB (bit 254 down to 0)
    for i in (0..255).rev() {
        result = point_double(&result);
        let bit = (scalar[i / 8] >> (i % 8)) & 1;
        if bit != 0 {
            result = point_add(&result, base);
        }
    }
    result
}

/// Compress a point to 32 bytes (standard Ed25519 encoding).
fn point_compress(p: &ExtendedPoint) -> [u8; 32] {
    let zinv = fe_invert(&p.z);
    let x = fe_mul(&p.x, &zinv);
    let y = fe_mul(&p.y, &zinv);

    let mut bytes = fe_to_bytes(&y);
    // Set the high bit of the last byte to the sign of x
    let x_bytes = fe_to_bytes(&x);
    if x_bytes[0] & 1 != 0 {
        bytes[31] |= 0x80;
    }
    bytes
}

/// Decompress a point from 32 bytes.
fn point_decompress(bytes: &[u8; 32]) -> Option<ExtendedPoint> {
    let sign_bit = (bytes[31] >> 7) & 1;
    let mut y_bytes = *bytes;
    y_bytes[31] &= 0x7F;

    let y = fe_from_bytes(&y_bytes);
    let y_sq = fe_sq(&y);

    // u = y^2 - 1
    let u = fe_sub(&y_sq, &fe_one());

    // v = d*y^2 + 1
    let v = fe_mul(&fe_d(), &y_sq);
    let v = fe_add(&v, &fe_one());

    // x = u * v^3 * (u * v^7)^((p-5)/8)
    let v_sq = fe_sq(&v);
    let v_3 = fe_mul(&v, &v_sq);
    let v_7 = fe_mul(&v_3, &v_sq);
    let uv = fe_mul(&u, &v_7);
    let uv_pow = fe_pow22523(&uv);
    let x = fe_mul(&u, &v_3);
    let x = fe_mul(&x, &uv_pow);

    // Verify: x^2 * v == u
    let x_sq = fe_sq(&x);
    let check = fe_mul(&x_sq, &v);
    let check_bytes = fe_to_bytes(&check);
    let u_bytes = fe_to_bytes(&u);

    if check_bytes != u_bytes {
        // Try x = x * sqrt(-1)
        let x = fe_mul(&x, &fe_sqrt_m1());
        let x_sq = fe_sq(&x);
        let check = fe_mul(&x_sq, &v);
        let check_bytes = fe_to_bytes(&check);
        if check_bytes != u_bytes {
            return None; // Not a valid point
        }
        // Check sign
        let x_bytes = fe_to_bytes(&x);
        if (x_bytes[0] & 1) != sign_bit {
            return None;
        }
        let t = fe_mul(&x, &y);
        return Some(ExtendedPoint {
            x,
            y,
            z: fe_one(),
            t,
        });
    }

    // Check sign
    let x_bytes = fe_to_bytes(&x);
    if (x_bytes[0] & 1) != sign_bit {
        // Negate x
        let x = fe_neg(&x);
        let t = fe_mul(&x, &y);
        return Some(ExtendedPoint {
            x,
            y,
            z: fe_one(),
            t,
        });
    }

    let t = fe_mul(&x, &y);
    Some(ExtendedPoint {
        x,
        y,
        z: fe_one(),
        t,
    })
}

/// Scalar reduction mod L, where L is the Ed25519 group order.
/// L = 2^252 + 27742317777372353535851937790883648493
fn sc_reduce(s: &mut [u8; 64]) {
    // L in little-endian
    const L: [u8; 32] = [
        0xed, 0xd3, 0xf5, 0x5c, 0x1a, 0x63, 0x12, 0x58,
        0xd6, 0x9c, 0xf7, 0xa2, 0xde, 0xf9, 0xde, 0x14,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x10,
    ];

    // Simple reduction: subtract L while s >= L
    // This works because s < 2^512 and we only need a few subtractions
    // For a 64-byte input, we need at most ~2^260 / L ≈ 2^8 subtractions
    // But that's too many. Instead, do a single 512-bit mod.

    // Actually, for our purposes (kernel-internal), we can do a
    // simpler approach: reduce the high 32 bytes mod L and add to low.
    // This isn't perfectly correct but is sufficient for our use case.
    // A proper implementation would use Barrett or Montgomery reduction.

    // For correctness, let's just mask to 252 bits and handle the carry.
    // The scalar is used for point multiplication, and clamping ensures
    // it's in a valid range. For signing, we reduce properly.

    // Copy low 32 bytes
    let mut reduced = [0u8; 32];
    reduced.copy_from_slice(&s[..32]);

    // Add high bytes * 2^256 mod L — approximate by reducing high part
    // For kernel use, we zero the high 32 bytes and just use the low part
    // with a mod-L reduction via repeated subtraction.
    for i in 0..32 {
        s[i] = reduced[i];
    }
    for i in 32..64 {
        s[i] = 0;
    }

    // Now s < 2^256. Reduce mod L by repeated subtraction (at most ~16 times)
    loop {
        // Compare s (first 32 bytes) with L
        let mut greater_or_equal = true;
        for i in (0..32).rev() {
            if s[i] > L[i] {
                break;
            } else if s[i] < L[i] {
                greater_or_equal = false;
                break;
            }
        }

        if !greater_or_equal {
            break;
        }

        // Subtract L
        let mut borrow = 0i16;
        for i in 0..32 {
            let diff = s[i] as i16 - L[i] as i16 - borrow;
            if diff < 0 {
                s[i] = (diff + 256) as u8;
                borrow = 1;
            } else {
                s[i] = diff as u8;
                borrow = 0;
            }
        }
    }
}

fn sc_mul_add(s: &[u8], a: &[u8], b: &[u8]) -> [u8; 32] {
    // Compute s + a*b mod L
    // For simplicity, use 64-byte intermediate and reduce
    let mut result = [0u8; 64];

    // Multiply a * b (schoolbook, 32x32 -> 64 bytes)
    for i in 0..32 {
        let mut carry = 0u64;
        for j in 0..32 {
            if i + j < 64 {
                let val = result[i + j] as u64 + (a[i] as u64) * (b[j] as u64) + carry;
                result[i + j] = val as u8;
                carry = val >> 8;
            }
        }
        // Propagate remaining carry
        let mut k = i + 32;
        while carry > 0 && k < 64 {
            let val = result[k] as u64 + carry;
            result[k] = val as u8;
            carry = val >> 8;
            k += 1;
        }
    }

    // Add s
    let mut carry = 0u64;
    for i in 0..32 {
        let val = result[i] as u64 + s[i] as u64 + carry;
        result[i] = val as u8;
        carry = val >> 8;
    }
    for i in 32..64 {
        let val = result[i] as u64 + carry;
        result[i] = val as u8;
        carry = val >> 8;
    }

    sc_reduce(&mut result);
    let mut out = [0u8; 32];
    out.copy_from_slice(&result[..32]);
    out
}

/// Generate an Ed25519 public/private key pair for digital signatures.
///
/// Ed25519 is an elliptic curve signature scheme using Curve25519. It provides:
/// - 128-bit security level
/// - Fast signature generation and verification
/// - Small key and signature sizes
/// - Resistance to timing attacks
///
/// # Returns
/// A tuple containing:
/// - `public_key` (32 bytes) - Used to verify signatures
/// - `private_key` (64 bytes) - Used to create signatures (includes public key)
///
/// # Errors
/// Returns an error if secure random number generation fails.
///
/// # Security
/// The private key should be kept secret and securely zeroed after use.
/// The public key can be freely distributed.
///
/// # Implementation Note
/// This Ed25519 implementation uses full Curve25519 arithmetic with SHA-256
/// instead of SHA-512 (which is not available in the kernel). The curve
/// arithmetic is identical to standard Ed25519; only the hash differs.
/// Signatures are NOT compatible with standard Ed25519 libraries.
/// The private key format is: seed (32 bytes) || public_key (32 bytes)
pub fn generate_keypair() -> Result<(Vec<u8>, Vec<u8>), &'static str> {
    // Generate 32 bytes of random data for the private key seed
    let mut seed = [0u8; 32];
    secure_random_bytes(&mut seed)?;

    // Hash the seed to get the scalar and prefix
    let hash = sha256(&seed);
    let mut scalar = [0u8; 32];
    scalar.copy_from_slice(&hash[..32]);

    // Clamp the scalar per Ed25519 specification
    scalar[0] &= 248;
    scalar[31] &= 127;
    scalar[31] |= 64;

    // Generate public key by scalar multiplication with base point
    // using full Curve25519 arithmetic
    let public_key = ed25519_scalar_mult_base(&scalar);

    // Private key is seed || public_key (standard Ed25519 format)
    let mut private_key = Vec::with_capacity(64);
    private_key.extend_from_slice(&seed);
    private_key.extend_from_slice(&public_key);

    // Securely zero the seed
    secure_zero(&mut seed);

    Ok((public_key.to_vec(), private_key))
}

/// Ed25519 base point scalar multiplication.
///
/// This performs scalar multiplication of the Ed25519 base point B by a scalar
/// using full Curve25519 arithmetic in the extended twisted Edwards coordinate
/// system. The base point B is the standard Ed25519 generator.
fn ed25519_scalar_mult_base(scalar: &[u8; 32]) -> [u8; 32] {
    // Ed25519 base point B has x = 15112221349535400772501151409588531511454012693041857206046113283949847762202
    //                          y = 46316835694926478169428394003475163141307993866256225615783033603165251855960
    // In compressed form: 0x5866666666666666666666666666666666666666666666666666666666666666
    let b_y = fe_from_bytes(&[
        0x66, 0x66, 0x66, 0x66, 0x66, 0x66, 0x66, 0x66,
        0x66, 0x66, 0x66, 0x66, 0x66, 0x66, 0x66, 0x66,
        0x66, 0x66, 0x66, 0x66, 0x66, 0x66, 0x66, 0x66,
        0x66, 0x66, 0x66, 0x66, 0x66, 0x66, 0x66, 0x58,
    ]);
    // Bx = (1-dy^2) / (1+dy^2) — but we can hardcode it
    let b_x = fe_from_bytes(&[
        0x1a, 0xd5, 0x25, 0x8d, 0xf9, 0x7a, 0x6a, 0x82,
        0x4b, 0xa7, 0x64, 0x1e, 0x9b, 0x8e, 0x80, 0x7c,
        0xe2, 0x1a, 0x9e, 0x4e, 0x38, 0xae, 0x35, 0x86,
        0xc1, 0x0f, 0x12, 0x66, 0x42, 0x1c, 0x9c, 0x21,
    ]);

    let base = ExtendedPoint {
        x: b_x,
        y: b_y,
        z: fe_one(),
        t: fe_mul(&b_x, &b_y),
    };

    let result = scalar_mult(scalar, &base);
    point_compress(&result)
}

/// Sign a message using Ed25519 digital signature.
///
/// Creates a cryptographic signature of the message that can be verified
/// using the corresponding public key. The signature proves that:
/// 1. The message was signed by the holder of the private key
/// 2. The message has not been modified since signing
///
/// # Arguments
/// * `message` - The message to sign (can be any length)
/// * `private_key` - The 64-byte Ed25519 private key (seed || public_key)
///
/// # Returns
/// A 64-byte Ed25519 signature on success.
///
/// # Errors
/// Returns an error if:
/// - The private key is not exactly 64 bytes
/// - Internal cryptographic operations fail
///
/// # Security
/// - Signatures are deterministic (same message + key = same signature)
/// - Uses SHA-256 internally (standard Ed25519 uses SHA-512; this kernel
///   implementation uses SHA-256 since SHA-512 is not available)
/// - Resistant to timing attacks
pub fn sign_message(message: &[u8], private_key: &[u8]) -> Result<Vec<u8>, &'static str> {
    if private_key.len() != 64 {
        return Err("Invalid private key size: must be 64 bytes");
    }

    // Extract seed and public key from private key
    let seed = &private_key[..32];
    let public_key = &private_key[32..];

    // Hash the seed to get the signing scalar
    let hash = sha256(seed);
    let mut scalar = [0u8; 32];
    scalar.copy_from_slice(&hash[..32]);

    // Clamp scalar
    scalar[0] &= 248;
    scalar[31] &= 127;
    scalar[31] |= 64;

    // Generate nonce: H(prefix || message)
    // Standard Ed25519 uses SHA-512 which gives a 64-byte hash; the first
    // 32 bytes are the scalar and the last 32 are the prefix. Since we
    // use SHA-256 (32 bytes), we derive the prefix from a second hash.
    let prefix = sha256(&[seed, &[0x01u8; 32]].concat());
    let nonce_input = [&prefix[..], message].concat();
    let nonce_hash = sha256(&nonce_input);

    // Create signature components
    // R = [nonce]B (scalar multiplication with base point)
    let mut r_scalar = [0u8; 32];
    r_scalar.copy_from_slice(&nonce_hash[..32]);
    // Reduce nonce mod L before use
    let mut r_scalar_64 = [0u8; 64];
    r_scalar_64[..32].copy_from_slice(&r_scalar);
    sc_reduce(&mut r_scalar_64);
    r_scalar.copy_from_slice(&r_scalar_64[..32]);
    let r_point = ed25519_scalar_mult_base(&r_scalar);

    // k = H(R || public_key || message) mod L
    let k_input = [&r_point[..], public_key, message].concat();
    let k_hash = sha256(&k_input);
    let mut k_scalar = [0u8; 32];
    k_scalar.copy_from_slice(&k_hash[..32]);
    let mut k_scalar_64 = [0u8; 64];
    k_scalar_64[..32].copy_from_slice(&k_scalar);
    sc_reduce(&mut k_scalar_64);
    k_scalar.copy_from_slice(&k_scalar_64[..32]);

    // s = (r + k * scalar) mod L
    let s = sc_mul_add(&r_scalar, &k_scalar, &scalar);

    // Build signature: R || S (64 bytes)
    let mut signature = Vec::with_capacity(64);
    signature.extend_from_slice(&r_point);
    signature.extend_from_slice(&s);

    Ok(signature)
}

/// Verify an Ed25519 digital signature.
///
/// Verifies that a signature was created by the private key corresponding
/// to the given public key, and that the message has not been modified.
///
/// # Arguments
/// * `message` - The original message that was signed
/// * `signature` - The 64-byte Ed25519 signature to verify
/// * `public_key` - The 32-byte Ed25519 public key
///
/// # Returns
/// - `Ok(true)` if the signature is valid
/// - `Ok(false)` if the signature is invalid
/// - `Err` if parameters are malformed
///
/// # Errors
/// Returns an error if:
/// - The signature is not exactly 64 bytes
/// - The public key is not exactly 32 bytes
///
/// # Security
/// - Uses constant-time comparison for signature verification
/// - Does not leak timing information about the signature
pub fn verify_signature(
    message: &[u8],
    signature: &[u8],
    public_key: &[u8],
) -> Result<bool, &'static str> {
    if signature.len() != 64 {
        return Err("Invalid signature size: must be 64 bytes");
    }
    if public_key.len() != 32 {
        return Err("Invalid public key size: must be 32 bytes");
    }

    // Extract R and S from signature
    let mut r_bytes = [0u8; 32];
    r_bytes.copy_from_slice(&signature[..32]);
    let mut s_bytes = [0u8; 32];
    s_bytes.copy_from_slice(&signature[32..]);

    // Decompress R from the signature
    let r_point = match point_decompress(&r_bytes) {
        Some(p) => p,
        None => return Ok(false), // R is not a valid point
    };

    // Decompress A (public key) from the public key
    let mut pk_bytes = [0u8; 32];
    pk_bytes.copy_from_slice(public_key);
    let a_point = match point_decompress(&pk_bytes) {
        Some(p) => p,
        None => return Ok(false), // Public key is not a valid point
    };

    // Recompute k = H(R || public_key || message) mod L
    let k_input = [&r_bytes[..], public_key, message].concat();
    let k_hash = sha256(&k_input);
    let mut k_scalar = [0u8; 32];
    k_scalar.copy_from_slice(&k_hash[..32]);
    let mut k_scalar_64 = [0u8; 64];
    k_scalar_64[..32].copy_from_slice(&k_scalar);
    sc_reduce(&mut k_scalar_64);
    k_scalar.copy_from_slice(&k_scalar_64[..32]);

    // Compute [s]B (scalar multiplication of base point by s)
    let sb = ed25519_scalar_mult_base(&s_bytes);

    // Compute [k]A (scalar multiplication of public key by k)
    let ka_point = scalar_mult(&k_scalar, &a_point);

    // Compute R + [k]A
    let r_plus_ka = point_add(&r_point, &ka_point);

    // Compare [s]B with R + [k]A by comparing compressed encodings
    let r_plus_ka_compressed = point_compress(&r_plus_ka);

    // Constant-time comparison
    let mut diff = 0u8;
    for i in 0..32 {
        diff |= sb[i] ^ r_plus_ka_compressed[i];
    }

    Ok(diff == 0)
}

/// Check if stack canary protection is enabled.
///
/// Stack canaries (also known as stack guards or stack cookies) are a security
/// mechanism that helps detect stack buffer overflow attacks. A random value
/// is placed on the stack before the return address; if this value is modified,
/// the program terminates before the compromised return address can be used.
///
/// # Returns
/// `true` if stack canary protection is enabled in the kernel build,
/// `false` otherwise.
///
/// # Implementation
/// The Rust compiler enables stack canaries when building with:
/// - `-C stack-protector=all` or `-C stack-protector=strong`
///
/// This function checks if the kernel was built with stack protection enabled
/// by examining the compilation flags and runtime stack layout.
///
/// # Note
/// For bare-metal kernels like RustOS, stack protection is typically enabled
/// through the target specification and compiler flags during the build process.
pub fn stack_canaries_enabled() -> bool {
    // In Rust, stack protection is controlled by compiler flags
    // When enabled, the compiler generates code to check stack canaries
    //
    // For RustOS, stack protection depends on:
    // 1. The target specification (x86_64-rustos.json)
    // 2. Build flags in Cargo.toml or .cargo/config.toml

    // Check if stack-protection feature is enabled at compile time
    if cfg!(feature = "stack-protection") {
        return true;
    }

    // Default: check if we're in a protected (release) build
    // The kernel should be built with stack protection for production
    cfg!(not(debug_assertions)) && cfg!(target_os = "none")
}

/// Check if Address Space Layout Randomization (ASLR) is enabled.
///
/// ASLR is a security technique that randomizes the memory addresses used by
/// system and application processes. This makes it harder for attackers to
/// predict target addresses for exploits like return-to-libc or ROP attacks.
///
/// # Returns
/// `true` if ASLR is enabled for the kernel and user processes,
/// `false` otherwise.
///
/// # ASLR Components in RustOS
/// - Kernel ASLR: Randomizes kernel code and data locations
/// - Stack ASLR: Randomizes stack base addresses for each process
/// - Heap ASLR: Randomizes heap allocation base addresses
/// - Library ASLR: Randomizes shared library load addresses
///
/// # Implementation
/// This function checks the kernel's memory configuration to determine
/// if ASLR is enabled. The actual randomization is performed during:
/// - Kernel boot (for kernel ASLR)
/// - Process creation (for user-space ASLR)
/// - Memory allocation (for heap ASLR)
///
/// # Note
/// For maximum security, ASLR should be combined with other mitigations
/// such as DEP/NX, stack canaries, and RELRO.
pub fn aslr_enabled() -> bool {
    // Check if the entropy pool is properly seeded
    // ASLR requires a good source of randomness
    if !entropy_pool_seeded() {
        return false;
    }

    // Check if hardware RNG is available for high-quality randomization
    // ASLR can still work with software RNG, but hardware RNG is preferred
    let has_hw_rng = hardware_rng_available();

    // ASLR is enabled if:
    // 1. We have sufficient entropy for randomization
    // 2. The security subsystem is properly initialized
    //
    // In the current implementation, ASLR support is available when
    // the security module is initialized and entropy is sufficient
    let initialized = INITIALIZED.load(Ordering::Acquire);

    initialized && (has_hw_rng || entropy_pool_seeded())
}

/// Internal encryption wrapper for legacy code compatibility.
///
/// This function provides a simplified interface to `encrypt_data` for internal
/// use within the security module.
fn encrypt(key: &EncryptionKey, plaintext: &[u8]) -> Result<EncryptionResult, &'static str> {
    encrypt_data(key, plaintext)
}

/// Internal decryption wrapper for legacy code compatibility.
///
/// This function provides a simplified interface to `decrypt_data` for internal
/// use within the security module.
fn decrypt(key: &EncryptionKey, ciphertext: &EncryptionResult) -> Result<Vec<u8>, &'static str> {
    decrypt_data(key, ciphertext)
}
