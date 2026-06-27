// RustOS Error Handling and Recovery System
// Comprehensive error handling with graceful recovery mechanisms

use alloc::string::String;
use alloc::vec::Vec;
use core::fmt;
use lazy_static::lazy_static;
use spin::{Mutex, RwLock};

/// Global kernel error types
#[derive(Debug, Clone)]
pub enum KernelError {
    Hardware(HardwareError),
    Memory(MemoryError),
    Process(ProcessError),
    Network(NetworkError),
    FileSystem(FileSystemError),
    Security(SecurityError),
    System(SystemError),
}

#[derive(Debug, Clone)]
pub enum HardwareError {
    DeviceNotFound,
    InitializationFailed,
    CommunicationTimeout,
    InvalidConfiguration,
    InterruptHandlingFailed,
    PowerManagementFailed,
    TemperatureExceeded,
    HardwareFault,
}

#[derive(Debug, Clone)]
pub enum MemoryError {
    OutOfMemory,
    InvalidAddress,
    MappingFailed,
    PermissionDenied,
    Fragmentation,
    CorruptionDetected,
    PageFaultUnrecoverable,
}

#[derive(Debug, Clone)]
pub enum ProcessError {
    NotFound,
    AlreadyExists,
    InvalidState,
    ResourceExhausted,
    PermissionDenied,
    DeadlockDetected,
    StackOverflow,
}

#[derive(Debug, Clone)]
pub enum NetworkError {
    ConnectionRefused,
    ConnectionReset,
    Timeout,
    InvalidPacket,
    BufferFull,
    DeviceError,
    ProtocolError,
}

#[derive(Debug, Clone)]
pub enum FileSystemError {
    FileNotFound,
    PermissionDenied,
    DiskFull,
    CorruptedData,
    InvalidPath,
    DeviceError,
    QuotaExceeded,
}

#[derive(Debug, Clone)]
pub enum SecurityError {
    AccessDenied,
    InvalidCredentials,
    PrivilegeEscalation,
    BufferOverflow,
    IntegrityViolation,
    CryptographicFailure,
}

#[derive(Debug, Clone)]
pub enum SystemError {
    ResourceExhausted,
    ServiceUnavailable,
    ConfigurationError,
    InternalError,
    NotImplemented,
    Timeout,
}

/// Error severity levels
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ErrorSeverity {
    Info,
    Warning,
    Error,
    Critical,
    Fatal,
}

/// Recovery action types
#[derive(Debug, Clone)]
pub enum RecoveryAction {
    Retry,
    Fallback,
    Restart,
    Isolate,
    Shutdown,
    None,
}

/// Error context with recovery information
#[derive(Debug, Clone)]
pub struct ErrorContext {
    pub error: KernelError,
    pub severity: ErrorSeverity,
    pub location: &'static str,
    pub message: String,
    pub recovery_action: RecoveryAction,
    pub retry_count: u32,
    pub timestamp: u64,
}

impl ErrorContext {
    pub fn new(
        error: KernelError,
        severity: ErrorSeverity,
        location: &'static str,
        message: String,
    ) -> Self {
        Self {
            error,
            severity,
            location,
            message,
            recovery_action: RecoveryAction::None,
            retry_count: 0,
            timestamp: crate::time::get_system_time_ms(),
        }
    }

    pub fn with_recovery(mut self, action: RecoveryAction) -> Self {
        self.recovery_action = action;
        self
    }

    pub fn increment_retry(&mut self) {
        self.retry_count += 1;
    }
}

impl fmt::Display for KernelError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            KernelError::Hardware(e) => write!(f, "Hardware Error: {:?}", e),
            KernelError::Memory(e) => write!(f, "Memory Error: {:?}", e),
            KernelError::Process(e) => write!(f, "Process Error: {:?}", e),
            KernelError::Network(e) => write!(f, "Network Error: {:?}", e),
            KernelError::FileSystem(e) => write!(f, "FileSystem Error: {:?}", e),
            KernelError::Security(e) => write!(f, "Security Error: {:?}", e),
            KernelError::System(e) => write!(f, "System Error: {:?}", e),
        }
    }
}

/// Global error recovery manager
pub struct ErrorRecoveryManager {
    error_history: Vec<ErrorContext>,
    recovery_strategies: RwLock<Vec<RecoveryStrategy>>,
    health_monitor: HealthMonitor,
}

#[derive(Debug, Clone)]
pub struct RecoveryStrategy {
    pub error_pattern: fn(&KernelError) -> bool,
    pub recovery_fn: fn(&mut ErrorContext) -> Result<(), KernelError>,
    pub max_retries: u32,
    pub cooldown_ms: u64,
}

/// System health monitoring
pub struct HealthMonitor {
    error_counts: [u32; 7], // One for each error type
    last_error_time: [u64; 7],
    system_health_score: u8, // 0-100
    critical_threshold: u32,
}

impl HealthMonitor {
    pub fn new() -> Self {
        Self {
            error_counts: [0; 7],
            last_error_time: [0; 7],
            system_health_score: 100,
            critical_threshold: 10,
        }
    }

    pub fn record_error(&mut self, error: &KernelError) {
        let index = match error {
            KernelError::Hardware(_) => 0,
            KernelError::Memory(_) => 1,
            KernelError::Process(_) => 2,
            KernelError::Network(_) => 3,
            KernelError::FileSystem(_) => 4,
            KernelError::Security(_) => 5,
            KernelError::System(_) => 6,
        };

        self.error_counts[index] += 1;
        self.last_error_time[index] = crate::time::get_system_time_ms();
        self.update_health_score();
    }

    fn update_health_score(&mut self) {
        let total_errors: u32 = self.error_counts.iter().sum();
        let current_time = crate::time::get_system_time_ms();

        // Decay old errors (older than 1 minute)
        for i in 0..7 {
            if current_time - self.last_error_time[i] > 60000 {
                self.error_counts[i] = self.error_counts[i].saturating_sub(1);
            }
        }

        // Calculate health score based on recent error frequency
        self.system_health_score = if total_errors == 0 {
            100
        } else {
            (100 - (total_errors * 10).min(100)) as u8
        };
    }

    pub fn is_system_healthy(&self) -> bool {
        self.system_health_score > 50
    }

    pub fn get_health_score(&self) -> u8 {
        self.system_health_score
    }
}

impl ErrorRecoveryManager {
    pub fn new() -> Self {
        Self {
            error_history: Vec::new(),
            recovery_strategies: RwLock::new(Vec::new()),
            health_monitor: HealthMonitor::new(),
        }
    }

    pub fn handle_error(&mut self, mut context: ErrorContext) -> Result<(), KernelError> {
        // Record error for health monitoring
        self.health_monitor.record_error(&context.error);

        // Log error
        self.log_error(&context);

        // Attempt recovery based on severity
        match context.severity {
            ErrorSeverity::Info | ErrorSeverity::Warning => {
                // Log and continue
                Ok(())
            }
            ErrorSeverity::Error => self.attempt_recovery(&mut context),
            ErrorSeverity::Critical => self.attempt_critical_recovery(&mut context),
            ErrorSeverity::Fatal => self.handle_fatal_error(&context),
        }
    }

    fn attempt_recovery(&mut self, context: &mut ErrorContext) -> Result<(), KernelError> {
        let strategies = self.recovery_strategies.read();

        for strategy in strategies.iter() {
            if (strategy.error_pattern)(&context.error)
                && context.retry_count < strategy.max_retries
            {
                context.increment_retry();

                match (strategy.recovery_fn)(context) {
                    Ok(()) => {
                        crate::serial_println!("Recovery successful for error: {}", context.error);
                        return Ok(());
                    }
                    Err(e) => {
                        crate::serial_println!("Recovery attempt failed: {}", e);
                        continue;
                    }
                }
            }
        }

        // No recovery strategy worked
        Err(context.error.clone())
    }

    fn attempt_critical_recovery(&mut self, context: &mut ErrorContext) -> Result<(), KernelError> {
        crate::serial_println!("CRITICAL ERROR: {}", context.error);

        // Try standard recovery first
        if self.attempt_recovery(context).is_ok() {
            return Ok(());
        }

        // Critical recovery strategies
        match &context.error {
            KernelError::Memory(MemoryError::OutOfMemory) => {
                self.emergency_memory_cleanup();
                Ok(())
            }
            KernelError::Hardware(HardwareError::TemperatureExceeded) => {
                self.emergency_thermal_management();
                Ok(())
            }
            _ => {
                // Isolate the failing component if possible
                self.isolate_failing_component(context);
                Err(context.error.clone())
            }
        }
    }

    fn handle_fatal_error(&mut self, context: &ErrorContext) -> Result<(), KernelError> {
        crate::serial_println!("FATAL ERROR: System cannot continue");
        crate::serial_println!("Error: {}", context.error);
        crate::serial_println!("Location: {}", context.location);
        crate::serial_println!("Message: {}", context.message);

        // Save error information for post-mortem analysis
        self.save_crash_dump(context);

        // Attempt graceful shutdown
        self.graceful_shutdown();

        // If we reach here, force halt
        loop {
            unsafe {
                core::arch::asm!("hlt");
            }
        }
    }

    fn log_error(&mut self, context: &ErrorContext) {
        self.error_history.push(context.clone());

        // Keep only last 100 errors to prevent memory exhaustion
        if self.error_history.len() > 100 {
            self.error_history.remove(0);
        }

        crate::serial_println!(
            "[{}] {} at {}: {}",
            match context.severity {
                ErrorSeverity::Info => "INFO",
                ErrorSeverity::Warning => "WARN",
                ErrorSeverity::Error => "ERROR",
                ErrorSeverity::Critical => "CRITICAL",
                ErrorSeverity::Fatal => "FATAL",
            },
            context.error,
            context.location,
            context.message
        );
    }

    fn emergency_memory_cleanup(&mut self) {
        crate::serial_println!("Performing emergency memory cleanup");

        // Note: Emergency memory reclamation is not yet implemented.
        // Future implementation will include:
        //
        // 1. Cache Subsystem Cleanup:
        //    - Free buffer caches (page cache, directory cache, inode cache)
        //    - Drop non-essential memory-mapped files
        //    - Clear temporary data structures
        //
        // 2. Process Memory Management:
        //    - Terminate non-essential background processes (priority-based)
        //    - Request processes to release cached resources
        //    - Force garbage collection in managed processes
        //
        // 3. Memory Compaction:
        //    - Compact fragmented memory regions
        //    - Merge adjacent free blocks
        //    - Defragment heap allocations
        //
        // 4. Kernel Memory Optimization:
        //    - Release unused kernel memory pools
        //    - Trim oversized kernel buffers
        //    - Clear debugging and profiling data
        //
        // Requirements: Process priority system, cache subsystem, memory compaction algorithms
    }

    fn emergency_thermal_management(&mut self) {
        crate::serial_println!("Activating emergency thermal management");

        // Note: Thermal management is not yet implemented.
        // Future implementation will require:
        //
        // 1. ACPI Thermal Zone Support:
        //    - Parse ACPI thermal zone objects (_TMP, _CRT, _PSV, _HOT methods)
        //    - Monitor temperature sensors via ACPI
        //    - Implement critical temperature thresholds
        //
        // 2. CPU Frequency Scaling (P-states):
        //    - Detect CPU frequency capabilities via ACPI _PSS
        //    - Reduce CPU frequency to lower power states
        //    - Implement gradual throttling based on temperature
        //
        // 3. CPU Power States (C-states):
        //    - Enter deeper CPU sleep states when idle
        //    - Reduce active core count if supported
        //    - Coordinate with scheduler for core parking
        //
        // 4. Fan Control:
        //    - Control fan speeds via ACPI _FST/_FSL methods
        //    - Use hardware-specific PWM control if available
        //    - Implement progressive fan speed curves
        //
        // 5. Workload Management:
        //    - Suspend non-critical background tasks
        //    - Defer schedulable operations
        //    - Throttle I/O operations
        //
        // Requirements: ACPI thermal extensions, CPUFREQ subsystem, hardware PWM control,
        //               advanced scheduler integration
    }

    fn isolate_failing_component(&mut self, context: &ErrorContext) {
        crate::serial_println!("Isolating failing component: {}", context.location);

        // Note: Component isolation is not yet implemented.
        // Future implementation will include:
        //
        // 1. Hardware Device Isolation:
        //    - Disable device via PCI configuration space (clear command register)
        //    - Remove device from active device registry
        //    - Unmap device memory-mapped I/O regions
        //    - Disable device interrupts via APIC/interrupt controller
        //
        // 2. I/O Operation Rerouting:
        //    - Identify backup/redundant devices (RAID, network failover)
        //    - Reroute pending I/O operations to functioning devices
        //    - Update I/O scheduler to exclude failed device
        //    - Notify filesystem layer of device unavailability
        //
        // 3. Component State Management:
        //    - Mark component as failed in device registry
        //    - Update system health monitoring status
        //    - Log failure event with diagnostic information
        //    - Prevent future initialization attempts
        //
        // 4. Dependent Subsystem Notification:
        //    - Notify all dependent subsystems of component failure
        //    - Trigger graceful degradation modes where applicable
        //    - Update system capabilities based on missing component
        //
        // 5. Recovery Preparation:
        //    - Prepare component for potential hot-unplug
        //    - Enable hot-plug detection for replacement device
        //    - Save component state for diagnostic purposes
        //
        // Requirements: Device hot-unplug support, I/O redirection framework,
        //               device registry with state tracking, subsystem notification system
    }

    fn save_crash_dump(&mut self, _context: &ErrorContext) {
        crate::serial_println!("Saving crash dump information");

        // Note: Crash dump saving is not yet implemented.
        // Future implementation will include:
        //
        // 1. CPU Register State Capture:
        //    - Save all general-purpose registers (RAX, RBX, RCX, RDX, RSI, RDI, RBP, RSP, R8-R15)
        //    - Save segment registers (CS, DS, SS, ES, FS, GS)
        //    - Save control registers (CR0, CR2, CR3, CR4, CR8)
        //    - Save debug registers (DR0-DR7)
        //    - Save MSRs (Model-Specific Registers) for debugging
        //    - Save RFLAGS and RIP at time of crash
        //
        // 2. Memory Dump:
        //    - Dump kernel memory space (full or selective)
        //    - Dump current process memory space
        //    - Dump stack frames for call stack reconstruction
        //    - Dump critical kernel data structures
        //    - Include memory map for crash analysis
        //
        // 3. Error Context and History:
        //    - Save complete error history log
        //    - Include recent system calls and their parameters
        //    - Save interrupt and exception history
        //    - Include recent kernel log messages
        //
        // 4. Process State Snapshot:
        //    - List of all running processes and their states
        //    - Process memory maps
        //    - Open file descriptors
        //    - Network connections
        //    - Pending I/O operations
        //
        // 5. System State Information:
        //    - Hardware configuration
        //    - Loaded drivers and modules
        //    - Mounted filesystems
        //    - Network configuration
        //    - System uptime and resource usage
        //
        // 6. Dump Storage:
        //    - Write to dedicated crash dump partition (if available)
        //    - Write to crash dump file on root filesystem
        //    - Compress dump data for space efficiency
        //    - Include metadata (timestamp, kernel version, hardware info)
        //
        // Requirements: Filesystem write support, dedicated dump partition,
        //               memory traversal capabilities, compression library
    }

    fn graceful_shutdown(&mut self) {
        crate::serial_println!("Initiating graceful shutdown");

        // Note: Graceful shutdown is not yet implemented.
        // Future implementation will include:
        //
        // 1. Process Notification and Termination:
        //    - Send SIGTERM to all user processes (allow graceful exit)
        //    - Wait for process termination with timeout (e.g., 10 seconds)
        //    - Send SIGKILL to remaining processes after timeout
        //    - Wait for all processes to fully terminate
        //    - Close all process file descriptors and free resources
        //
        // 2. Filesystem Synchronization:
        //    - Flush all dirty buffers to disk (page cache, directory cache)
        //    - Sync all mounted filesystems
        //    - Commit pending filesystem transactions
        //    - Update filesystem metadata (superblocks, inode tables)
        //    - Mark filesystems as cleanly unmounted
        //
        // 3. Filesystem Unmounting:
        //    - Unmount filesystems in reverse dependency order
        //    - Close all open file handles
        //    - Release filesystem resources
        //    - Unmount root filesystem last
        //
        // 4. Network Shutdown:
        //    - Close all network connections gracefully
        //    - Send FIN packets for TCP connections
        //    - Flush network buffers
        //    - Disable network interfaces
        //
        // 5. Device Driver Shutdown:
        //    - Stop all device drivers in dependency order
        //    - Flush device buffers
        //    - Disable device interrupts
        //    - Put devices in low-power or safe states
        //
        // 6. Hardware Power Management:
        //    - Disable APIC and interrupt controllers
        //    - Send ACPI shutdown command (via _PTS and _S5 methods)
        //    - Alternative: Use keyboard controller reset (0xFE to port 0x64)
        //    - Fallback: Triple-fault reboot if shutdown fails
        //
        // 7. Critical Data Preservation:
        //    - Save system logs to persistent storage
        //    - Save configuration changes
        //    - Update boot flags if necessary
        //
        // Requirements: Signal delivery system, filesystem sync implementation,
        //               ACPI power management, device driver shutdown hooks
    }

    pub fn register_recovery_strategy(&mut self, strategy: RecoveryStrategy) {
        let mut strategies = self.recovery_strategies.write();
        strategies.push(strategy);
    }

    pub fn get_system_health(&self) -> u8 {
        self.health_monitor.get_health_score()
    }

    pub fn get_error_history(&self) -> &[ErrorContext] {
        &self.error_history
    }
}

lazy_static! {
    pub static ref ERROR_MANAGER: Mutex<ErrorRecoveryManager> =
        Mutex::new(ErrorRecoveryManager::new());
}

/// Convenience macros for error handling
#[macro_export]
macro_rules! kernel_error {
    ($error:expr, $severity:expr, $msg:expr) => {
        $crate::error::ErrorContext::new(
            $error,
            $severity,
            concat!(file!(), ":", line!()),
            alloc::format!($msg),
        )
    };
    ($error:expr, $severity:expr, $msg:expr, $($arg:tt)*) => {
        $crate::error::ErrorContext::new(
            $error,
            $severity,
            concat!(file!(), ":", line!()),
            alloc::format!($msg, $($arg)*),
        )
    };
}

#[macro_export]
macro_rules! handle_error {
    ($error_context:expr) => {
        match $crate::error::ERROR_MANAGER
            .lock()
            .handle_error($error_context)
        {
            Ok(()) => {}
            Err(e) => {
                crate::serial_println!("Unrecoverable error: {}", e);
                return Err(e);
            }
        }
    };
}

#[macro_export]
macro_rules! try_with_recovery {
    ($expr:expr, $recovery:expr) => {
        match $expr {
            Ok(val) => val,
            Err(e) => {
                let context = $crate::kernel_error!(
                    e,
                    $crate::error::ErrorSeverity::Error,
                    "Operation failed"
                )
                .with_recovery($recovery);
                $crate::handle_error!(context);
                return Err(e);
            }
        }
    };
}

/// Initialize the error handling system
pub fn init_error_handling() {
    crate::serial_println!("Initializing error handling and recovery system");

    // Register default recovery strategies
    let mut manager = ERROR_MANAGER.lock();

    // Memory error recovery
    manager.register_recovery_strategy(RecoveryStrategy {
        error_pattern: |e| matches!(e, KernelError::Memory(MemoryError::OutOfMemory)),
        recovery_fn: |_| {
            // Note: Detailed memory cleanup not yet implemented.
            // Future implementation will include:
            // - Per-process resource cleanup (free cached data, close unused handles)
            // - Orphan resource detection and cleanup
            // - Memory leak prevention and detection
            // - Slab allocator defragmentation
            // - Page cache pressure relief
            //
            // For now, recovery attempts are logged but no cleanup is performed.
            crate::serial_println!("Memory recovery: cleanup not yet implemented");
            Ok(())
        },
        max_retries: 3,
        cooldown_ms: 1000,
    });

    // Hardware error recovery
    manager.register_recovery_strategy(RecoveryStrategy {
        error_pattern: |e| {
            matches!(
                e,
                KernelError::Hardware(HardwareError::CommunicationTimeout)
            )
        },
        recovery_fn: |_| {
            // Note: Hardware reset sequence not yet implemented.
            // Future implementation will include:
            // - Soft device reset via PCI configuration space
            // - Device-specific reset commands (NVMe admin reset, AHCI HBA reset, etc.)
            // - Bus reset for PCI/PCIe devices
            // - Full device re-initialization after reset
            // - Verify device functionality post-reset
            //
            // Alternative reset methods:
            // - ACPI reset (_RST method)
            // - Keyboard controller reset (port 0x64, command 0xFE)
            // - Triple-fault reset (last resort)
            //
            // For now, recovery attempts are logged but no reset is performed.
            crate::serial_println!("Hardware recovery: reset not yet implemented");
            Ok(())
        },
        max_retries: 5,
        cooldown_ms: 500,
    });

    crate::serial_println!("Error handling system initialized");
}
