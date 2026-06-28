// RustOS Comprehensive Logging and Debugging System
// Structured logging with multiple output targets and debugging interfaces

use alloc::boxed::Box;
use alloc::collections::VecDeque;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use core::fmt;
use lazy_static::lazy_static;
use spin::Mutex;

/// Log levels for structured logging
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[repr(u8)]
pub enum LogLevel {
    Trace = 0,
    Debug = 1,
    Info = 2,
    Warn = 3,
    Error = 4,
    Fatal = 5,
}

impl LogLevel {
    pub fn as_str(&self) -> &'static str {
        match self {
            LogLevel::Trace => "TRACE",
            LogLevel::Debug => "DEBUG",
            LogLevel::Info => "INFO",
            LogLevel::Warn => "WARN",
            LogLevel::Error => "ERROR",
            LogLevel::Fatal => "FATAL",
        }
    }

    pub fn color_code(&self) -> &'static str {
        match self {
            LogLevel::Trace => "\x1b[37m", // White
            LogLevel::Debug => "\x1b[36m", // Cyan
            LogLevel::Info => "\x1b[32m",  // Green
            LogLevel::Warn => "\x1b[33m",  // Yellow
            LogLevel::Error => "\x1b[31m", // Red
            LogLevel::Fatal => "\x1b[35m", // Magenta
        }
    }
}

impl fmt::Display for LogLevel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// Log entry structure
#[derive(Debug, Clone)]
pub struct LogEntry {
    pub timestamp: u64,
    pub level: LogLevel,
    pub module: String,
    pub message: String,
    pub file: Option<String>,
    pub line: Option<u32>,
    pub thread_id: Option<u32>,
}

impl LogEntry {
    pub fn new(level: LogLevel, module: &str, message: String) -> Self {
        Self {
            timestamp: crate::time::get_system_time_ms(),
            level,
            module: module.to_string(),
            message,
            file: None,
            line: None,
            thread_id: None,
        }
    }

    pub fn with_location(mut self, file: &str, line: u32) -> Self {
        self.file = Some(file.to_string());
        self.line = Some(line);
        self
    }

    pub fn with_thread(mut self, thread_id: u32) -> Self {
        self.thread_id = Some(thread_id);
        self
    }
}

impl fmt::Display for LogEntry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let uptime_ms = crate::time::uptime_ms();
        let uptime_sec = uptime_ms / 1000;
        let uptime_ms_part = uptime_ms % 1000;

        write!(f, "[{:6}.{:03}] ", uptime_sec, uptime_ms_part)?;
        write!(f, "{}{:5}\x1b[0m ", self.level.color_code(), self.level)?;
        write!(f, "{:12} ", self.module)?;

        if let Some(ref file) = self.file {
            if let Some(line) = self.line {
                write!(f, "{}:{} ", file, line)?;
            }
        }

        if let Some(thread_id) = self.thread_id {
            write!(f, "[T{}] ", thread_id)?;
        }

        write!(f, "{}", self.message)
    }
}

/// Log output targets
pub trait LogOutput: Send + Sync {
    fn write_log(&mut self, entry: &LogEntry);
    fn flush(&mut self);
    fn name(&self) -> &str;
}

/// Serial port log output
pub struct SerialLogOutput {
    name: String,
}

impl SerialLogOutput {
    pub fn new() -> Self {
        Self {
            name: "Serial".to_string(),
        }
    }
}

impl LogOutput for SerialLogOutput {
    fn write_log(&mut self, entry: &LogEntry) {
        crate::serial_println!("{}", entry);
    }

    fn flush(&mut self) {
        // Serial output is immediate
    }

    fn name(&self) -> &str {
        &self.name
    }
}

/// VGA buffer log output
pub struct VgaLogOutput {
    name: String,
}

impl VgaLogOutput {
    pub fn new() -> Self {
        Self {
            name: "VGA".to_string(),
        }
    }
}

impl LogOutput for VgaLogOutput {
    fn write_log(&mut self, entry: &LogEntry) {
        // Only show important messages on VGA to avoid spam
        if entry.level >= LogLevel::Info {
            crate::println!("{}", entry);
        }
    }

    fn flush(&mut self) {
        // VGA output is immediate
    }

    fn name(&self) -> &str {
        &self.name
    }
}

/// Memory buffer log output for debugging
pub struct MemoryLogOutput {
    name: String,
    buffer: VecDeque<LogEntry>,
    max_entries: usize,
}

impl MemoryLogOutput {
    pub fn new(max_entries: usize) -> Self {
        Self {
            name: "Memory".to_string(),
            buffer: VecDeque::with_capacity(max_entries),
            max_entries,
        }
    }

    pub fn get_entries(&self) -> Vec<LogEntry> {
        self.buffer.iter().cloned().collect()
    }

    pub fn clear(&mut self) {
        self.buffer.clear();
    }
}

impl LogOutput for MemoryLogOutput {
    fn write_log(&mut self, entry: &LogEntry) {
        if self.buffer.len() >= self.max_entries {
            self.buffer.pop_front();
        }
        self.buffer.push_back(entry.clone());
    }

    fn flush(&mut self) {
        // Memory buffer doesn't need flushing
    }

    fn name(&self) -> &str {
        &self.name
    }
}

/// Main logging system
pub struct Logger {
    outputs: Vec<Box<dyn LogOutput>>,
    min_level: LogLevel,
    enabled: bool,
}

impl Logger {
    pub fn new() -> Self {
        Self {
            outputs: Vec::new(),
            min_level: LogLevel::Info,
            enabled: true,
        }
    }

    pub fn add_output(&mut self, output: Box<dyn LogOutput>) {
        self.outputs.push(output);
    }

    pub fn set_min_level(&mut self, level: LogLevel) {
        self.min_level = level;
    }

    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }

    pub fn log(&mut self, entry: LogEntry) {
        if !self.enabled || entry.level < self.min_level {
            return;
        }

        for output in &mut self.outputs {
            output.write_log(&entry);
        }
    }

    pub fn flush(&mut self) {
        for output in &mut self.outputs {
            output.flush();
        }
    }

    pub fn get_memory_logs(&self) -> Vec<LogEntry> {
        for output in &self.outputs {
            if output.name() == "Memory" {
                if let Some(memory_output) = output.as_any().downcast_ref::<MemoryLogOutput>() {
                    return memory_output.get_entries();
                }
            }
        }
        Vec::new()
    }
}

// Add trait for downcasting
trait AsAny {
    fn as_any(&self) -> &dyn core::any::Any;
}

impl<T: 'static> AsAny for T {
    fn as_any(&self) -> &dyn core::any::Any {
        self
    }
}

lazy_static! {
    pub static ref LOGGER: Mutex<Logger> = Mutex::new(Logger::new());
}

/// Initialize the logging system
pub fn init_logging() {
    let mut logger = LOGGER.lock();

    // Add serial output
    logger.add_output(Box::new(SerialLogOutput::new()));

    // Add VGA output
    logger.add_output(Box::new(VgaLogOutput::new()));

    // Add memory buffer for debugging (keep last 1000 entries)
    logger.add_output(Box::new(MemoryLogOutput::new(1000)));

    // Set default log level
    logger.set_min_level(LogLevel::Debug);

    crate::serial_println!("Logging system initialized with multiple outputs");
}

/// Log a message with specified level
pub fn log(level: LogLevel, module: &str, message: String) {
    let entry = LogEntry::new(level, module, message);
    LOGGER.lock().log(entry);
}

/// Log a message with location information
pub fn log_with_location(level: LogLevel, module: &str, message: String, file: &str, line: u32) {
    let entry = LogEntry::new(level, module, message).with_location(file, line);
    LOGGER.lock().log(entry);
}

/// Set minimum log level
pub fn set_log_level(level: LogLevel) {
    LOGGER.lock().set_min_level(level);
}

/// Enable or disable logging
pub fn set_logging_enabled(enabled: bool) {
    LOGGER.lock().set_enabled(enabled);
}

/// Get recent log entries from memory buffer
pub fn get_recent_logs() -> Vec<LogEntry> {
    LOGGER.lock().get_memory_logs()
}

/// Flush all log outputs
pub fn flush_logs() {
    LOGGER.lock().flush();
}

/// Logging macros
#[macro_export]
macro_rules! log_trace {
    ($module:expr, $($arg:tt)*) => {
        $crate::logging::log_with_location(
            $crate::logging::LogLevel::Trace,
            $module,
            alloc::format!($($arg)*),
            file!(),
            line!()
        );
    };
}

#[macro_export]
macro_rules! log_debug {
    ($module:expr, $($arg:tt)*) => {
        $crate::logging::log_with_location(
            $crate::logging::LogLevel::Debug,
            $module,
            alloc::format!($($arg)*),
            file!(),
            line!()
        );
    };
}

#[macro_export]
macro_rules! log_info {
    ($module:expr, $($arg:tt)*) => {
        $crate::logging::log_with_location(
            $crate::logging::LogLevel::Info,
            $module,
            alloc::format!($($arg)*),
            file!(),
            line!()
        );
    };
}

#[macro_export]
macro_rules! log_warn {
    ($module:expr, $($arg:tt)*) => {
        $crate::logging::log_with_location(
            $crate::logging::LogLevel::Warn,
            $module,
            alloc::format!($($arg)*),
            file!(),
            line!()
        );
    };
}

#[macro_export]
macro_rules! log_error {
    ($module:expr, $($arg:tt)*) => {
        $crate::logging::log_with_location(
            $crate::logging::LogLevel::Error,
            $module,
            alloc::format!($($arg)*),
            file!(),
            line!()
        );
    };
}

#[macro_export]
macro_rules! log_fatal {
    ($module:expr, $($arg:tt)*) => {
        $crate::logging::log_with_location(
            $crate::logging::LogLevel::Fatal,
            $module,
            alloc::format!($($arg)*),
            file!(),
            line!()
        );
    };
}

/// Performance monitoring and profiling
pub mod profiling {
    use super::*;
    use alloc::collections::BTreeMap;

    /// Performance counter
    #[derive(Debug, Clone)]
    pub struct PerfCounter {
        pub name: String,
        pub count: u64,
        pub total_time_ns: u64,
        pub min_time_ns: u64,
        pub max_time_ns: u64,
        pub avg_time_ns: u64,
    }

    impl PerfCounter {
        pub fn new(name: String) -> Self {
            Self {
                name,
                count: 0,
                total_time_ns: 0,
                min_time_ns: u64::MAX,
                max_time_ns: 0,
                avg_time_ns: 0,
            }
        }

        pub fn record(&mut self, time_ns: u64) {
            self.count += 1;
            self.total_time_ns += time_ns;
            self.min_time_ns = self.min_time_ns.min(time_ns);
            self.max_time_ns = self.max_time_ns.max(time_ns);
            self.avg_time_ns = self.total_time_ns / self.count;
        }
    }

    lazy_static! {
        static ref PERF_COUNTERS: Mutex<BTreeMap<String, PerfCounter>> =
            Mutex::new(BTreeMap::new());
    }

    /// Performance measurement timer
    pub struct PerfTimer {
        name: String,
        start_time: crate::time::Timer,
    }

    impl PerfTimer {
        pub fn new(name: String) -> Self {
            Self {
                name,
                start_time: crate::time::Timer::new(),
            }
        }

        pub fn finish(self) {
            let elapsed_ns = self.start_time.elapsed_ns();
            let mut counters = PERF_COUNTERS.lock();

            let counter = counters
                .entry(self.name.clone())
                .or_insert_with(|| PerfCounter::new(self.name));

            counter.record(elapsed_ns);
        }
    }

    /// Start performance measurement
    pub fn start_measurement(name: &str) -> PerfTimer {
        PerfTimer::new(name.to_string())
    }

    /// Get performance statistics
    pub fn get_perf_stats() -> Vec<PerfCounter> {
        PERF_COUNTERS.lock().values().cloned().collect()
    }

    /// Reset performance counters
    pub fn reset_perf_stats() {
        PERF_COUNTERS.lock().clear();
    }

    /// Display performance statistics
    pub fn display_perf_stats() {
        let stats = get_perf_stats();

        log_info!("profiling", "=== PERFORMANCE STATISTICS ===");
        log_info!(
            "profiling",
            "{:<30} {:>8} {:>12} {:>12} {:>12} {:>12}",
            "Function",
            "Count",
            "Total(μs)",
            "Min(μs)",
            "Max(μs)",
            "Avg(μs)"
        );

        for stat in stats {
            log_info!(
                "profiling",
                "{:<30} {:>8} {:>12} {:>12} {:>12} {:>12}",
                stat.name,
                stat.count,
                stat.total_time_ns / 1000,
                stat.min_time_ns / 1000,
                stat.max_time_ns / 1000,
                stat.avg_time_ns / 1000
            );
        }

        log_info!("profiling", "=== END PERFORMANCE STATISTICS ===");
    }
}

/// Debugging interfaces and tools
pub mod debug {
    use super::*;

    /// Debug command handler
    pub trait DebugCommand {
        fn name(&self) -> &str;
        fn description(&self) -> &str;
        fn execute(&self, args: &[&str]) -> Result<String, String>;
    }

    /// Memory debug command
    pub struct MemoryDebugCommand;

    impl DebugCommand for MemoryDebugCommand {
        fn name(&self) -> &str {
            "memory"
        }

        fn description(&self) -> &str {
            "Display memory statistics and usage"
        }

        fn execute(&self, _args: &[&str]) -> Result<String, String> {
            if let Ok(stats) = crate::memory_basic::get_memory_stats() {
                Ok(alloc::format!(
                    "Memory Stats:\n  Total: {} MB\n  Usable: {} MB\n  Regions: {}",
                    stats.total_memory / (1024 * 1024),
                    stats.usable_memory / (1024 * 1024),
                    stats.memory_regions
                ))
            } else {
                Err("Memory statistics not available".to_string())
            }
        }
    }

    /// Health debug command
    pub struct HealthDebugCommand;

    impl DebugCommand for HealthDebugCommand {
        fn name(&self) -> &str {
            "health"
        }

        fn description(&self) -> &str {
            "Display system health information"
        }

        fn execute(&self, _args: &[&str]) -> Result<String, String> {
            let diagnostics = crate::health::get_system_diagnostics();
            Ok(alloc::format!(
                "System Health:\n  Status: {:?}\n  Score: {}/100\n  CPU: {}%\n  Memory: {}%\n  Errors: {}",
                diagnostics.health_status,
                diagnostics.metrics.health_score,
                diagnostics.metrics.cpu_usage,
                diagnostics.metrics.memory_usage,
                diagnostics.total_errors
            ))
        }
    }

    /// Interrupt debug command
    pub struct InterruptDebugCommand;

    impl DebugCommand for InterruptDebugCommand {
        fn name(&self) -> &str {
            "interrupts"
        }

        fn description(&self) -> &str {
            "Display interrupt statistics"
        }

        fn execute(&self, _args: &[&str]) -> Result<String, String> {
            let stats = crate::interrupts::get_interrupt_stats();
            Ok(alloc::format!(
                "Interrupt Stats:\n  Timer: {}\n  Keyboard: {}\n  Exceptions: {}\n  Page Faults: {}\n  Spurious: {}",
                stats.timer_count,
                stats.keyboard_count,
                stats.exception_count,
                stats.page_fault_count,
                stats.spurious_count
            ))
        }
    }

    /// Log debug command
    pub struct LogDebugCommand;

    impl DebugCommand for LogDebugCommand {
        fn name(&self) -> &str {
            "logs"
        }

        fn description(&self) -> &str {
            "Display recent log entries"
        }

        fn execute(&self, args: &[&str]) -> Result<String, String> {
            let count = if args.len() > 0 {
                args[0].parse::<usize>().unwrap_or(10)
            } else {
                10
            };

            let logs = get_recent_logs();
            let recent_logs: Vec<_> = logs.iter().rev().take(count).collect();

            let mut result = alloc::format!("Recent {} log entries:\n", recent_logs.len());
            for log in recent_logs.iter().rev() {
                result.push_str(&alloc::format!("{}\n", log));
            }

            Ok(result)
        }
    }

    lazy_static! {
        static ref DEBUG_COMMANDS: Mutex<Vec<Box<dyn DebugCommand + Send + Sync>>> = {
            let mut commands: Vec<Box<dyn DebugCommand + Send + Sync>> = Vec::new();
            commands.push(Box::new(MemoryDebugCommand));
            commands.push(Box::new(HealthDebugCommand));
            commands.push(Box::new(InterruptDebugCommand));
            commands.push(Box::new(LogDebugCommand));
            Mutex::new(commands)
        };
    }

    /// Execute a debug command
    pub fn execute_debug_command(command: &str, args: &[&str]) -> Result<String, String> {
        let commands = DEBUG_COMMANDS.lock();

        for cmd in commands.iter() {
            if cmd.name() == command {
                return cmd.execute(args);
            }
        }

        Err(alloc::format!("Unknown command: {}", command))
    }

    /// List available debug commands
    pub fn list_debug_commands() -> Vec<(String, String)> {
        let commands = DEBUG_COMMANDS.lock();
        commands
            .iter()
            .map(|cmd| (cmd.name().to_string(), cmd.description().to_string()))
            .collect()
    }

    /// Add a custom debug command
    pub fn add_debug_command(command: Box<dyn DebugCommand + Send + Sync>) {
        DEBUG_COMMANDS.lock().push(command);
    }

    /// Debug console interface — reads commands from the keyboard.
    ///
    /// Reads characters from the PS/2 keyboard handler, assembles them
    /// into a line, and dispatches to the appropriate debug command.
    pub fn debug_console() {
        log_info!("debug", "Debug console started. Type 'help' for commands.");

        // Display available commands
        let commands = list_debug_commands();
        log_info!("debug", "Available commands:");
        for (name, desc) in commands {
            log_info!("debug", "  {}: {}", name, desc);
        }

        // Read and process commands from the keyboard
        let mut line = [0u8; 256];
        let mut pos = 0;

        loop {
            if let Some(ch) = crate::keyboard::read_char() {
                if ch == '\n' || ch == '\r' {
                    // Process the command
                    if pos > 0 {
                        let cmd = core::str::from_utf8(&line[..pos]).unwrap_or("");
                        log_info!("debug", "Command: {}", cmd);

                        // Dispatch to debug commands
                        match cmd {
                            "help" => {
                                let commands = list_debug_commands();
                                log_info!("debug", "Available commands:");
                                for (name, desc) in commands {
                                    log_info!("debug", "  {}: {}", name, desc);
                                }
                            }
                            "stats" => {
                                log_info!("debug", "Use 'dump' for kernel state dump");
                            }
                            "dump" => {
                                kernel_debug::dump_kernel_state();
                            }
                            "exit" => {
                                log_info!("debug", "Exiting debug console");
                                break;
                            }
                            _ => {
                                log_info!("debug", "Unknown command: {}", cmd);
                            }
                        }
                    }
                    pos = 0;
                } else if ch == '\x08' || ch == '\x7F' {
                    // Backspace
                    if pos > 0 {
                        pos -= 1;
                    }
                } else if pos < line.len() && ch.is_ascii_graphic() || ch == ' ' {
                    line[pos] = ch as u8;
                    pos += 1;
                }
            }
        }
    }
}

/// Kernel debugging utilities
pub mod kernel_debug {

    /// Dump kernel state for debugging
    pub fn dump_kernel_state() {
        log_info!("kernel", "=== KERNEL STATE DUMP ===");

        // System uptime
        let uptime = crate::time::uptime_ms();
        log_info!(
            "kernel",
            "Uptime: {}.{:03} seconds",
            uptime / 1000,
            uptime % 1000
        );

        // Timer information
        let timer_stats = crate::time::get_timer_stats();
        log_info!(
            "kernel",
            "Timer: {:?}, TSC: {} Hz",
            timer_stats.active_timer,
            timer_stats.tsc_frequency
        );

        // Memory information
        if let Ok(mem_stats) = crate::memory_basic::get_memory_stats() {
            log_info!(
                "kernel",
                "Memory: {} MB total, {} MB usable",
                mem_stats.total_memory / (1024 * 1024),
                mem_stats.usable_memory / (1024 * 1024)
            );
        }

        // Health information
        let health = crate::health::get_health_status();
        log_info!("kernel", "System Health: {:?}", health);

        // Interrupt statistics
        let int_stats = crate::interrupts::get_interrupt_stats();
        log_info!(
            "kernel",
            "Interrupts: {} total",
            int_stats.timer_count + int_stats.keyboard_count + int_stats.exception_count
        );

        log_info!("kernel", "=== END KERNEL STATE DUMP ===");
    }

    /// Validate kernel subsystems
    pub fn validate_kernel_subsystems() -> bool {
        log_info!("kernel", "Validating kernel subsystems...");

        let mut all_valid = true;

        // Check timer system
        if !crate::time::is_initialized() {
            log_error!("kernel", "Timer system not initialized");
            all_valid = false;
        } else {
            log_debug!("kernel", "Timer system: OK");
        }

        // Check interrupt system
        if !crate::interrupts::are_enabled() {
            log_warn!("kernel", "Interrupts are disabled");
        } else {
            log_debug!("kernel", "Interrupt system: OK");
        }

        // Check health monitoring
        if !crate::health::is_system_healthy() {
            log_warn!("kernel", "System health is poor");
        } else {
            log_debug!("kernel", "Health monitoring: OK");
        }

        if all_valid {
            log_info!("kernel", "All kernel subsystems validated successfully");
        } else {
            log_error!("kernel", "Some kernel subsystems failed validation");
        }

        all_valid
    }
}

/// Initialize all logging and debugging systems
pub fn init_logging_and_debugging() {
    init_logging();

    log_info!("system", "Logging and debugging system initialized");
    log_info!(
        "system",
        "Available log levels: TRACE, DEBUG, INFO, WARN, ERROR, FATAL"
    );
    log_info!("system", "Performance profiling enabled");
    log_info!("system", "Debug console available");

    // Test the logging system
    log_debug!("system", "Debug logging test");
    log_trace!("system", "Trace logging test");

    // Interactive debug console blocks boot waiting for keyboard input.
    if crate::boot_ui::boot_config().interactive_debug_console {
        debug::debug_console();
    } else {
        log_info!("system", "Debug console available (non-interactive boot)");
    }
}
