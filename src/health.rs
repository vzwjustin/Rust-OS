// RustOS System Health Monitoring and Diagnostics
// Provides comprehensive system health monitoring and automatic recovery

use crate::error::{ErrorContext, ErrorSeverity, KernelError, ERROR_MANAGER};
use alloc::string::{String, ToString};
use alloc::vec;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use lazy_static::lazy_static;
use spin::RwLock;

/// Read a 64-bit model-specific register.
///
/// # Safety
/// `rdmsr` is a privileged ring-0 instruction. The caller must ensure `msr`
/// is a valid MSR index for the running CPU model and that the read has no
/// harmful side effects.
#[inline(always)]
unsafe fn rdmsr(msr: u32) -> u64 {
    let (low, high): (u32, u32);
    core::arch::asm!(
        "rdmsr",
        out("eax") low,
        out("edx") high,
        in("ecx") msr,
        options(nostack, preserves_flags),
    );
    ((high as u64) << 32) | (low as u64)
}

/// System health metrics
#[derive(Debug, Clone)]
pub struct SystemHealthMetrics {
    pub cpu_usage: u8,           // 0-100%
    pub memory_usage: u8,        // 0-100%
    pub error_rate: u32,         // Errors per minute
    pub uptime_seconds: u64,     // System uptime
    pub temperature: Option<u8>, // CPU temperature in Celsius
    pub health_score: u8,        // Overall health 0-100
    pub last_update: u64,        // Timestamp of last update
}

/// Health monitoring thresholds
#[derive(Debug, Clone)]
pub struct HealthThresholds {
    pub critical_cpu_usage: u8,    // 95%
    pub critical_memory_usage: u8, // 90%
    pub critical_error_rate: u32,  // 100 errors/min
    pub critical_temperature: u8,  // 85°C
    pub warning_cpu_usage: u8,     // 80%
    pub warning_memory_usage: u8,  // 75%
    pub warning_error_rate: u32,   // 50 errors/min
    pub warning_temperature: u8,   // 75°C
}

impl Default for HealthThresholds {
    fn default() -> Self {
        Self {
            critical_cpu_usage: 95,
            critical_memory_usage: 90,
            critical_error_rate: 100,
            critical_temperature: 85,
            warning_cpu_usage: 80,
            warning_memory_usage: 75,
            warning_error_rate: 50,
            warning_temperature: 75,
        }
    }
}

/// Health status levels
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HealthStatus {
    Excellent, // 90-100
    Good,      // 70-89
    Fair,      // 50-69
    Poor,      // 30-49
    Critical,  // 0-29
}

impl HealthStatus {
    pub fn from_score(score: u8) -> Self {
        match score {
            90..=100 => HealthStatus::Excellent,
            70..=89 => HealthStatus::Good,
            50..=69 => HealthStatus::Fair,
            30..=49 => HealthStatus::Poor,
            _ => HealthStatus::Critical,
        }
    }

    /// Get the numeric health score (0.0 to 1.0)
    pub fn overall_health(&self) -> f32 {
        match self {
            HealthStatus::Excellent => 0.95,
            HealthStatus::Good => 0.80,
            HealthStatus::Fair => 0.60,
            HealthStatus::Poor => 0.40,
            HealthStatus::Critical => 0.20,
        }
    }
}

/// System component health tracking
#[derive(Debug, Clone)]
pub struct ComponentHealth {
    pub name: String,
    pub status: HealthStatus,
    pub last_error: Option<String>,
    pub error_count: u32,
    pub last_check: u64,
    pub enabled: bool,
}

/// Health monitoring system
pub struct HealthMonitor {
    metrics: RwLock<SystemHealthMetrics>,
    thresholds: RwLock<HealthThresholds>,
    components: RwLock<Vec<ComponentHealth>>,
    monitoring_enabled: AtomicBool,
    last_health_check: AtomicU64,
    last_interrupt_count: AtomicU64,
    health_check_interval: AtomicU64, // milliseconds
}

impl HealthMonitor {
    pub fn new() -> Self {
        Self {
            metrics: RwLock::new(SystemHealthMetrics {
                cpu_usage: 0,
                memory_usage: 0,
                error_rate: 0,
                uptime_seconds: 0,
                temperature: None,
                health_score: 100,
                last_update: 0,
            }),
            thresholds: RwLock::new(HealthThresholds::default()),
            components: RwLock::new(Vec::new()),
            monitoring_enabled: AtomicBool::new(true),
            last_health_check: AtomicU64::new(0),
            last_interrupt_count: AtomicU64::new(0),
            health_check_interval: AtomicU64::new(5000), // 5 seconds
        }
    }

    pub fn init(&self) {
        self.register_core_components();
        self.monitoring_enabled.store(true, Ordering::Relaxed);
        crate::serial_println!("Health monitoring system initialized");
    }

    fn register_core_components(&self) {
        let mut components = self.components.write();

        let core_components = vec![
            "Memory Manager",
            "Process Scheduler",
            "Interrupt Handler",
            "Timer System",
            "Network Stack",
            "File System",
            "Hardware Drivers",
            "Security System",
        ];

        for name in core_components {
            components.push(ComponentHealth {
                name: name.to_string(),
                status: HealthStatus::Excellent,
                last_error: None,
                error_count: 0,
                last_check: crate::time::get_system_time_ms(),
                enabled: true,
            });
        }
    }

    pub fn update_metrics(&self) {
        if !self.monitoring_enabled.load(Ordering::Relaxed) {
            return;
        }

        let current_time = crate::time::get_system_time_ms();
        let last_check = self.last_health_check.load(Ordering::Relaxed);
        let interval = self.health_check_interval.load(Ordering::Relaxed);

        if current_time - last_check < interval {
            return; // Too soon for next check
        }

        let mut metrics = self.metrics.write();

        // Update basic metrics
        metrics.uptime_seconds = crate::time::uptime_ms() / 1000;
        metrics.last_update = current_time;

        // Update CPU usage (simplified - would need performance counters in real implementation)
        metrics.cpu_usage = self.estimate_cpu_usage();

        // Update memory usage
        metrics.memory_usage = self.get_memory_usage();

        // Update error rate
        metrics.error_rate = self.calculate_error_rate();

        // Update temperature (if available)
        metrics.temperature = self.read_cpu_temperature();

        // Calculate overall health score
        metrics.health_score = self.calculate_health_score(&metrics);

        self.last_health_check
            .store(current_time, Ordering::Relaxed);

        // Check for critical conditions
        self.check_critical_conditions(&metrics);
    }

    fn estimate_cpu_usage(&self) -> u8 {
        let interrupt_count = crate::interrupts::get_interrupt_count();
        let previous_count = self
            .last_interrupt_count
            .swap(interrupt_count, Ordering::Relaxed);
        let current_time = crate::time::get_system_time_ms();
        let previous_time = self.last_health_check.load(Ordering::Relaxed);

        if previous_count == 0 || current_time <= previous_time {
            return 0;
        }

        let elapsed_ms = current_time - previous_time;
        let interrupts = interrupt_count.saturating_sub(previous_count);

        // Treat one interrupt per millisecond as saturated CPU activity. This is still
        // only a coarse health signal, but it is based on rate instead of counter value.
        ((interrupts.saturating_mul(100)) / elapsed_ms.max(1)).min(100) as u8
    }

    fn get_memory_usage(&self) -> u8 {
        // Get memory statistics from memory manager
        if let Ok(stats) = crate::memory_basic::get_memory_stats() {
            let used = stats.total_memory - stats.usable_memory;
            ((used * 100) / stats.total_memory.max(1)) as u8
        } else {
            50 // Default estimate if stats unavailable
        }
    }

    fn calculate_error_rate(&self) -> u32 {
        // Get error rate from error manager
        if let Some(manager) = ERROR_MANAGER.try_lock() {
            let history = manager.get_error_history();
            let current_time = crate::time::get_system_time_ms();

            // Count errors in the last minute
            let recent_errors = history
                .iter()
                .filter(|e| current_time - e.timestamp < 60000)
                .count();

            recent_errors as u32
        } else {
            0
        }
    }

    fn read_cpu_temperature(&self) -> Option<u8> {
        // Read CPU temperature via the IA32_THERM_STATUS MSR (0x19C).
        //
        // On Intel CPUs that support the digital thermal sensor (DTS), bits
        // 22:16 of IA32_THERM_STATUS contain the "digital temperature
        // reading" — the number of degrees Celsius below TjMax (the maximum
        // junction temperature). The actual temperature is:
        //
        //     temperature = TjMax - digital_reading
        //
        // TjMax is model-specific (read from IA32_TEMPERATURE_TARGET MSR
        // 0x1A2, bits 23:16 on most recent CPUs). We read it when available
        // and fall back to a conservative 100 °C assumption used by many
        // Intel desktop/server parts.
        //
        // If the DTS is not supported (the MSR read returns 0 in bits 22:16
        // and the VALID bit 31 is clear), we return None.

        /// IA32_THERM_STATUS
        const MSR_THERM_STATUS: u32 = 0x19C;
        /// IA32_TEMPERATURE_TARGET
        const MSR_TEMP_TARGET: u32 = 0x1A2;
        /// Typical TjMax fallback (°C)
        const TJMAX_FALLBACK: u8 = 100;

        // SAFETY: rdmsr is a privileged instruction that reads a model-specific
        // register. We only read thermal-status registers that are read-only
        // and have no side effects. The CPU must be in ring 0 (kernel mode).
        let therm_status = unsafe { rdmsr(MSR_THERM_STATUS) };

        // Bit 31 = VALID (thermal status valid); if clear, DTS not available.
        if (therm_status >> 31) & 1 == 0 {
            return None;
        }

        // Digital temperature reading: bits 22:16
        let digital_reading = ((therm_status >> 16) & 0x7F) as u8;
        if digital_reading == 0 {
            // 0 means "at or above TjMax" — report TjMax.
            return Some(TJMAX_FALLBACK);
        }

        // Determine TjMax from IA32_TEMPERATURE_TARGET if available.
        let tjmax = match unsafe { rdmsr(MSR_TEMP_TARGET) } {
            target if target != 0 => {
                // Bits 23:16 on most modern Intel CPUs.
                let t = ((target >> 16) & 0xFF) as u8;
                if t >= 70 && t <= 130 {
                    t
                } else {
                    TJMAX_FALLBACK
                }
            }
            _ => TJMAX_FALLBACK,
        };

        let temp = tjmax.saturating_sub(digital_reading);
        if temp > 0 && temp <= tjmax {
            Some(temp)
        } else {
            None
        }
    }

    fn calculate_health_score(&self, metrics: &SystemHealthMetrics) -> u8 {
        let thresholds = self.thresholds.read();
        let mut score = 100u8;

        // CPU usage impact
        if metrics.cpu_usage >= thresholds.critical_cpu_usage {
            score = score.saturating_sub(30);
        } else if metrics.cpu_usage >= thresholds.warning_cpu_usage {
            score = score.saturating_sub(15);
        }

        // Memory usage impact
        if metrics.memory_usage >= thresholds.critical_memory_usage {
            score = score.saturating_sub(25);
        } else if metrics.memory_usage >= thresholds.warning_memory_usage {
            score = score.saturating_sub(10);
        }

        // Error rate impact
        if metrics.error_rate >= thresholds.critical_error_rate {
            score = score.saturating_sub(40);
        } else if metrics.error_rate >= thresholds.warning_error_rate {
            score = score.saturating_sub(20);
        }

        // Temperature impact
        if let Some(temp) = metrics.temperature {
            if temp >= thresholds.critical_temperature {
                score = score.saturating_sub(35);
            } else if temp >= thresholds.warning_temperature {
                score = score.saturating_sub(15);
            }
        }

        // Component health impact
        let components = self.components.read();
        let critical_components = components
            .iter()
            .filter(|c| c.status == HealthStatus::Critical)
            .count();
        let poor_components = components
            .iter()
            .filter(|c| c.status == HealthStatus::Poor)
            .count();

        score = score.saturating_sub((critical_components * 20) as u8);
        score = score.saturating_sub((poor_components * 10) as u8);

        score
    }

    fn check_critical_conditions(&self, metrics: &SystemHealthMetrics) {
        let thresholds = self.thresholds.read();

        // Check CPU usage
        if metrics.cpu_usage >= thresholds.critical_cpu_usage {
            self.handle_critical_condition("CPU usage critical", metrics.cpu_usage as u32);
        }

        // Check memory usage
        if metrics.memory_usage >= thresholds.critical_memory_usage {
            self.handle_critical_condition("Memory usage critical", metrics.memory_usage as u32);
        }

        // Check error rate
        if metrics.error_rate >= thresholds.critical_error_rate {
            self.handle_critical_condition("Error rate critical", metrics.error_rate);
        }

        // Check temperature
        if let Some(temp) = metrics.temperature {
            if temp >= thresholds.critical_temperature {
                self.handle_critical_condition("Temperature critical", temp as u32);
            }
        }

        // Check overall health
        if metrics.health_score < 30 {
            self.handle_critical_condition("System health critical", metrics.health_score as u32);
        }
    }

    fn handle_critical_condition(&self, condition: &str, value: u32) {
        crate::serial_println!("CRITICAL CONDITION: {} (value: {})", condition, value);

        let error_context = ErrorContext::new(
            KernelError::System(crate::error::SystemError::ResourceExhausted),
            ErrorSeverity::Critical,
            "health_monitor",
            alloc::format!("{}: {}", condition, value),
        );

        if let Some(mut manager) = ERROR_MANAGER.try_lock() {
            let _ = manager.handle_error(error_context);
        }
    }

    pub fn update_component_health(
        &self,
        component_name: &str,
        status: HealthStatus,
        error: Option<String>,
    ) {
        let mut components = self.components.write();

        if let Some(component) = components.iter_mut().find(|c| c.name == component_name) {
            component.status = status;
            component.last_check = crate::time::get_system_time_ms();

            if error.is_some() {
                component.error_count += 1;
                component.last_error = error;
            }
        }
    }

    pub fn get_health_metrics(&self) -> SystemHealthMetrics {
        self.metrics.read().clone()
    }

    pub fn get_health_status(&self) -> HealthStatus {
        let metrics = self.metrics.read();
        HealthStatus::from_score(metrics.health_score)
    }

    pub fn get_component_health(&self) -> Vec<ComponentHealth> {
        self.components.read().clone()
    }

    pub fn set_monitoring_enabled(&self, enabled: bool) {
        self.monitoring_enabled.store(enabled, Ordering::Relaxed);
    }

    pub fn is_monitoring_enabled(&self) -> bool {
        self.monitoring_enabled.load(Ordering::Relaxed)
    }

    pub fn set_check_interval(&self, interval_ms: u64) {
        self.health_check_interval
            .store(interval_ms, Ordering::Relaxed);
    }

    pub fn get_system_diagnostics(&self) -> SystemDiagnostics {
        let metrics = self.get_health_metrics();
        let components = self.get_component_health();
        let error_history = if let Some(manager) = ERROR_MANAGER.try_lock() {
            manager.get_error_history().len()
        } else {
            0
        };

        SystemDiagnostics {
            health_status: HealthStatus::from_score(metrics.health_score),
            metrics,
            components,
            total_errors: error_history,
            monitoring_enabled: self.is_monitoring_enabled(),
        }
    }
}

/// Complete system diagnostics
#[derive(Debug, Clone)]
pub struct SystemDiagnostics {
    pub health_status: HealthStatus,
    pub metrics: SystemHealthMetrics,
    pub components: Vec<ComponentHealth>,
    pub total_errors: usize,
    pub monitoring_enabled: bool,
}

lazy_static! {
    pub static ref HEALTH_MONITOR: HealthMonitor = HealthMonitor::new();
}

/// Initialize the health monitoring system
pub fn init_health_monitoring() {
    HEALTH_MONITOR.init();

    // Schedule periodic health checks
    let _timer_id = crate::time::schedule_periodic_timer(5000000, health_check_callback); // 5 seconds

    crate::serial_println!("Health monitoring system initialized with 5-second intervals");
}

/// Periodic health check callback
fn health_check_callback() {
    HEALTH_MONITOR.update_metrics();
}

/// Update component health status
pub fn update_component_health(component: &str, status: HealthStatus, error: Option<String>) {
    HEALTH_MONITOR.update_component_health(component, status, error);
}

/// Get current system health metrics
pub fn get_health_metrics() -> SystemHealthMetrics {
    HEALTH_MONITOR.get_health_metrics()
}

/// Get current system health status
pub fn get_health_status() -> HealthStatus {
    HEALTH_MONITOR.get_health_status()
}

/// Get complete system diagnostics
pub fn get_system_diagnostics() -> SystemDiagnostics {
    HEALTH_MONITOR.get_system_diagnostics()
}

/// Enable or disable health monitoring
pub fn set_monitoring_enabled(enabled: bool) {
    HEALTH_MONITOR.set_monitoring_enabled(enabled);
}

/// Check if system is healthy
pub fn is_system_healthy() -> bool {
    matches!(
        get_health_status(),
        HealthStatus::Excellent | HealthStatus::Good
    )
}

/// Display health information for debugging
pub fn display_health_info() {
    let diagnostics = get_system_diagnostics();

    crate::serial_println!("=== SYSTEM HEALTH DIAGNOSTICS ===");
    crate::serial_println!("Overall Status: {:?}", diagnostics.health_status);
    crate::serial_println!("Health Score: {}/100", diagnostics.metrics.health_score);
    crate::serial_println!("CPU Usage: {}%", diagnostics.metrics.cpu_usage);
    crate::serial_println!("Memory Usage: {}%", diagnostics.metrics.memory_usage);
    crate::serial_println!("Error Rate: {} errors/min", diagnostics.metrics.error_rate);
    crate::serial_println!("Uptime: {} seconds", diagnostics.metrics.uptime_seconds);

    if let Some(temp) = diagnostics.metrics.temperature {
        crate::serial_println!("CPU Temperature: {}°C", temp);
    }

    crate::serial_println!("Total Errors: {}", diagnostics.total_errors);
    crate::serial_println!(
        "Monitoring: {}",
        if diagnostics.monitoring_enabled {
            "Enabled"
        } else {
            "Disabled"
        }
    );

    crate::serial_println!("Component Health:");
    for component in &diagnostics.components {
        crate::serial_println!(
            "  {}: {:?} (errors: {})",
            component.name,
            component.status,
            component.error_count
        );
        if let Some(ref error) = component.last_error {
            crate::serial_println!("    Last error: {}", error);
        }
    }
    crate::serial_println!("=== END DIAGNOSTICS ===");
}

/// Macro for reporting component errors
#[macro_export]
macro_rules! report_component_error {
    ($component:expr, $error:expr) => {
        $crate::health::update_component_health(
            $component,
            $crate::health::HealthStatus::Poor,
            Some(alloc::format!($error))
        );
    };
    ($component:expr, $error:expr, $($arg:tt)*) => {
        $crate::health::update_component_health(
            $component,
            $crate::health::HealthStatus::Poor,
            Some(alloc::format!($error, $($arg)*))
        );
    };
}

/// Macro for reporting component recovery
#[macro_export]
macro_rules! report_component_recovery {
    ($component:expr) => {
        $crate::health::update_component_health(
            $component,
            $crate::health::HealthStatus::Good,
            None,
        );
    };
}
