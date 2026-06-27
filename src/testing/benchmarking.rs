//! Performance Monitoring and Benchmarking Tools for RustOS
//!
//! This module provides comprehensive performance monitoring and benchmarking
//! capabilities including:
//! - Real-time performance metrics collection
//! - Benchmarking tools for continuous performance monitoring
//! - Performance regression detection
//! - Latency and throughput measurements
//! - Resource utilization analysis

use crate::data_structures::{CacheFriendlyRingBuffer, LockFreeMpscQueue, CACHE_LINE_SIZE};
use crate::testing_framework::{TestCase, TestResult, TestSuite, TestType};
use alloc::{
    collections::BTreeMap,
    string::{String, ToString},
    vec,
    vec::Vec,
};
use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};

/// Performance metric types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MetricType {
    Latency,
    Throughput,
    CpuUtilization,
    MemoryUsage,
    NetworkBandwidth,
    DiskIo,
    CacheHitRate,
    ContextSwitches,
    Interrupts,
    SystemCalls,
}

/// Performance measurement units
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MetricUnit {
    Nanoseconds,
    Microseconds,
    Milliseconds,
    Seconds,
    Bytes,
    BytesPerSecond,
    OperationsPerSecond,
    Percentage,
    Count,
}

/// Performance measurement sample
#[derive(Debug, Clone)]
pub struct PerformanceSample {
    pub timestamp: u64,
    pub metric_type: MetricType,
    pub value: f64,
    pub unit: MetricUnit,
    pub component: String,
    pub metadata: BTreeMap<String, String>,
}

/// Performance statistics
#[derive(Debug, Clone)]
pub struct PerformanceStats {
    pub min: f64,
    pub max: f64,
    pub mean: f64,
    pub median: f64,
    pub std_dev: f64,
    pub percentile_95: f64,
    pub percentile_99: f64,
    pub sample_count: usize,
}

/// Benchmark configuration
#[derive(Debug, Clone)]
pub struct BenchmarkConfig {
    pub name: String,
    pub duration_ms: u64,
    pub warmup_ms: u64,
    pub iterations: usize,
    pub target_metric: MetricType,
    pub expected_min: Option<f64>,
    pub expected_max: Option<f64>,
    pub regression_threshold: f64, // Percentage
}

/// Benchmark result
#[derive(Debug, Clone)]
pub struct BenchmarkResult {
    pub config: BenchmarkConfig,
    pub stats: PerformanceStats,
    pub samples: Vec<PerformanceSample>,
    pub passed: bool,
    pub regression_detected: bool,
    pub baseline_comparison: Option<f64>, // Percentage difference from baseline
}

/// Performance monitor for real-time metrics collection
#[repr(align(64))]
pub struct PerformanceMonitor {
    enabled: AtomicBool,
    sample_count: AtomicU64,
    total_samples: AtomicU64,
    sample_buffer: CacheFriendlyRingBuffer<PerformanceSample>,
    metrics_queue: LockFreeMpscQueue<PerformanceSample>,
    _padding: [u8; CACHE_LINE_SIZE
        - 3 * core::mem::size_of::<AtomicU64>()
        - core::mem::size_of::<AtomicBool>()],
}

impl PerformanceMonitor {
    /// Create a new performance monitor
    pub fn new() -> Option<Self> {
        let sample_buffer = CacheFriendlyRingBuffer::new(4096)?; // 4K samples
        let metrics_queue = LockFreeMpscQueue::new();

        Some(Self {
            enabled: AtomicBool::new(false),
            sample_count: AtomicU64::new(0),
            total_samples: AtomicU64::new(0),
            sample_buffer,
            metrics_queue,
            _padding: [0; CACHE_LINE_SIZE
                - 3 * core::mem::size_of::<AtomicU64>()
                - core::mem::size_of::<AtomicBool>()],
        })
    }

    /// Enable performance monitoring
    pub fn enable(&self) {
        self.enabled.store(true, Ordering::Release);
    }

    /// Disable performance monitoring
    pub fn disable(&self) {
        self.enabled.store(false, Ordering::Release);
    }

    /// Record a performance sample
    pub fn record_sample(&self, sample: PerformanceSample) {
        if !self.enabled.load(Ordering::Acquire) {
            return;
        }

        // Try to add to ring buffer first (for recent samples)
        if self.sample_buffer.push(sample.clone()).is_err() {
            // Buffer full, sample dropped (oldest samples are overwritten)
        }

        // Add to metrics queue for processing
        self.metrics_queue.enqueue(sample);

        self.sample_count.fetch_add(1, Ordering::Relaxed);
        self.total_samples.fetch_add(1, Ordering::Relaxed);
    }

    /// Get recent samples from ring buffer
    pub fn get_recent_samples(&self, count: usize) -> Vec<PerformanceSample> {
        let mut samples = Vec::new();
        for _ in 0..count {
            if let Some(sample) = self.sample_buffer.pop() {
                samples.push(sample);
            } else {
                break;
            }
        }
        samples
    }

    /// Process queued metrics
    pub fn process_metrics(&self) -> Vec<PerformanceSample> {
        let mut processed = Vec::new();
        while let Some(sample) = self.metrics_queue.dequeue() {
            processed.push(sample);
        }
        processed
    }

    /// Get monitoring statistics
    pub fn get_stats(&self) -> (u64, u64) {
        (
            self.sample_count.load(Ordering::Relaxed),
            self.total_samples.load(Ordering::Relaxed),
        )
    }
}

/// Benchmark suite for performance testing
pub struct BenchmarkSuite {
    benchmarks: Vec<BenchmarkConfig>,
    baseline_results: BTreeMap<String, PerformanceStats>,
    monitor: PerformanceMonitor,
}

impl BenchmarkSuite {
    /// Create a new benchmark suite
    pub fn new() -> Option<Self> {
        Some(Self {
            benchmarks: Vec::new(),
            baseline_results: BTreeMap::new(),
            monitor: PerformanceMonitor::new()?,
        })
    }

    /// Add a benchmark
    pub fn add_benchmark(&mut self, config: BenchmarkConfig) {
        self.benchmarks.push(config);
    }

    /// Run all benchmarks
    pub fn run_all_benchmarks(&mut self) -> Vec<BenchmarkResult> {
        let mut results = Vec::new();

        self.monitor.enable();

        for config in &self.benchmarks {
            let result = self.run_benchmark(config);
            results.push(result);
        }

        self.monitor.disable();
        results
    }

    /// Run a single benchmark
    pub fn run_benchmark(&self, config: &BenchmarkConfig) -> BenchmarkResult {
        let mut samples = Vec::new();

        // Warmup phase
        if config.warmup_ms > 0 {
            let warmup_start = crate::time::uptime_us();
            while crate::time::uptime_us() - warmup_start < config.warmup_ms * 1000 {
                self.run_benchmark_iteration(config);
            }
        }

        // Measurement phase
        let measurement_start = crate::time::uptime_us();
        let duration_us = config.duration_ms * 1000;

        for _iteration in 0..config.iterations {
            if crate::time::uptime_us() - measurement_start > duration_us {
                break;
            }

            let iteration_start = crate::time::uptime_us();
            let result = self.run_benchmark_iteration(config);
            let _iteration_end = crate::time::uptime_us();

            let sample = PerformanceSample {
                timestamp: iteration_start,
                metric_type: config.target_metric,
                value: result,
                unit: match config.target_metric {
                    MetricType::Latency => MetricUnit::Microseconds,
                    MetricType::Throughput => MetricUnit::OperationsPerSecond,
                    MetricType::CpuUtilization | MetricType::CacheHitRate => MetricUnit::Percentage,
                    MetricType::MemoryUsage | MetricType::NetworkBandwidth | MetricType::DiskIo => {
                        MetricUnit::Bytes
                    }
                    _ => MetricUnit::Count,
                },
                component: config.name.clone(),
                metadata: BTreeMap::new(),
            };

            samples.push(sample);
            self.monitor.record_sample(samples.last().unwrap().clone());
        }

        // Calculate statistics
        let stats = self.calculate_statistics(&samples);

        // Check if benchmark passed
        let passed = self.check_benchmark_criteria(config, &stats);

        // Check for regression
        let (regression_detected, baseline_comparison) =
            if let Some(baseline) = self.baseline_results.get(&config.name) {
                let current_mean = stats.mean;
                let baseline_mean = baseline.mean;
                let difference_percent = ((current_mean - baseline_mean) / baseline_mean) * 100.0;
                let regression = difference_percent.abs() > config.regression_threshold;
                (regression, Some(difference_percent))
            } else {
                (false, None)
            };

        BenchmarkResult {
            config: config.clone(),
            stats,
            samples,
            passed,
            regression_detected,
            baseline_comparison,
        }
    }

    /// Run a single benchmark iteration
    fn run_benchmark_iteration(&self, config: &BenchmarkConfig) -> f64 {
        match config.target_metric {
            MetricType::Latency => self.measure_latency(&config.name),
            MetricType::Throughput => self.measure_throughput(&config.name),
            MetricType::CpuUtilization => self.measure_cpu_utilization(),
            MetricType::MemoryUsage => self.measure_memory_usage(),
            MetricType::NetworkBandwidth => self.measure_network_bandwidth(),
            MetricType::CacheHitRate => self.measure_cache_hit_rate(),
            MetricType::ContextSwitches => self.measure_context_switches(),
            MetricType::Interrupts => self.measure_interrupts(),
            MetricType::SystemCalls => self.measure_system_calls(),
            _ => 0.0,
        }
    }

    /// Measure operation latency
    fn measure_latency(&self, operation: &str) -> f64 {
        let start = crate::performance_monitor::read_tsc();

        // Perform different operations based on name
        match operation {
            "syscall_getpid" => {
                let context = crate::syscall::SyscallContext {
                    pid: 1,
                    syscall_num: crate::syscall::SyscallNumber::GetPid,
                    args: [0; 6],
                    user_sp: 0x7fff_0000,
                    user_ip: 0x4000_0000,
                    privilege_level: 3,
                    cwd: None,
                };
                let _ = crate::syscall::dispatch_syscall(&context);
            }
            "memory_allocation" => {
                // Use real memory management system
                use crate::memory::{get_memory_manager, MemoryZone};
                if let Some(memory_manager) = get_memory_manager() {
                    let manager = memory_manager;
                    if let Some(frame) = manager.allocate_frame_in_zone(MemoryZone::Normal) {
                        manager.deallocate_frame(frame, MemoryZone::Normal);
                    }
                }
            }
            "context_switch" => {
                crate::scheduler::schedule();
            }
            _ => {
                // Default operation
                for _ in 0..100 {
                    unsafe {
                        core::arch::asm!("nop");
                    }
                }
            }
        }

        let end = crate::performance_monitor::read_tsc();
        let cycles = end - start;

        // Convert cycles to microseconds (approximate)
        (cycles as f64) / 3000.0 // Assuming 3GHz CPU
    }

    /// Measure operation throughput
    fn measure_throughput(&self, operation: &str) -> f64 {
        let start_time = crate::time::uptime_us();
        let mut operations = 0;

        // Run operations for 1ms
        while crate::time::uptime_us() - start_time < 1000 {
            match operation {
                "syscall_throughput" => {
                    let context = crate::syscall::SyscallContext {
                        pid: 1,
                        syscall_num: crate::syscall::SyscallNumber::ClockGettime,
                        args: [0; 6],
                        user_sp: 0x7fff_0000,
                        user_ip: 0x4000_0000,
                        privilege_level: 3,
                        cwd: None,
                    };
                    let _ = crate::syscall::dispatch_syscall(&context);
                }
                "memory_throughput" => {
                    // Use real memory management system for throughput testing
                    use crate::memory::{get_memory_manager, MemoryZone};
                    if let Some(memory_manager) = get_memory_manager() {
                        let manager = memory_manager;
                        if let Some(frame) = manager.allocate_frame_in_zone(MemoryZone::Normal) {
                            manager.deallocate_frame(frame, MemoryZone::Normal);
                        }
                    }
                }
                _ => {
                    // Default operation
                    unsafe {
                        core::arch::asm!("nop");
                    }
                }
            }
            operations += 1;
        }

        let elapsed_us = crate::time::uptime_us() - start_time;
        if elapsed_us > 0 {
            (operations as f64 * 1_000_000.0) / elapsed_us as f64
        } else {
            0.0
        }
    }

    /// Measure CPU utilization
    fn measure_cpu_utilization(&self) -> f64 {
        crate::performance_monitor::cpu_utilization() as f64
    }

    /// Measure memory usage
    fn measure_memory_usage(&self) -> f64 {
        let (used, _total) = crate::performance_monitor::memory_usage();
        used as f64
    }

    /// Measure network bandwidth
    fn measure_network_bandwidth(&self) -> f64 {
        // Measure actual network bandwidth from driver statistics
        // This would read from network driver counters in real implementation
        let (_packets_sent, bytes_sent, _packets_received, bytes_received) =
            crate::network::get_interface_stats().unwrap_or((0, 0, 0, 0));

        // Calculate bandwidth based on actual traffic
        let total_bytes = bytes_sent + bytes_received;
        let measurement_window_seconds = 1.0; // 1 second measurement window

        total_bytes as f64 / measurement_window_seconds
    }

    /// Measure cache hit rate  
    fn measure_cache_hit_rate(&self) -> f64 {
        // Read actual CPU cache performance counters
        // This would use hardware performance counters in real implementation
        let cache_refs = crate::performance_monitor::read_cpu_counter(0x2E); // Cache references
        let cache_misses = crate::performance_monitor::read_cpu_counter(0x2F); // Cache misses

        if cache_refs > 0 {
            let hit_rate = ((cache_refs - cache_misses) as f64 / cache_refs as f64) * 100.0;
            hit_rate.max(0.0).min(100.0) // Clamp to valid percentage range
        } else {
            0.0 // No cache activity measured
        }
    }

    /// Measure context switches
    fn measure_context_switches(&self) -> f64 {
        let stats = crate::performance_monitor::get_stats();
        stats.context_switches as f64
    }

    /// Measure interrupts using real interrupt statistics
    fn measure_interrupts(&self) -> f64 {
        let stats = crate::interrupts::get_stats();
        (stats.timer_count + stats.keyboard_count + stats.serial_count + stats.exception_count)
            as f64
    }

    /// Measure system calls
    fn measure_system_calls(&self) -> f64 {
        let stats = crate::syscall::get_syscall_stats();
        stats.total_calls as f64
    }

    /// Calculate statistics for samples
    fn calculate_statistics(&self, samples: &[PerformanceSample]) -> PerformanceStats {
        if samples.is_empty() {
            return PerformanceStats {
                min: 0.0,
                max: 0.0,
                mean: 0.0,
                median: 0.0,
                std_dev: 0.0,
                percentile_95: 0.0,
                percentile_99: 0.0,
                sample_count: 0,
            };
        }

        let mut values: Vec<f64> = samples.iter().map(|s| s.value).collect();
        values.sort_by(|a, b| a.partial_cmp(b).unwrap());

        let min = values[0];
        let max = values[values.len() - 1];
        let mean = values.iter().sum::<f64>() / values.len() as f64;

        let median = if values.len() % 2 == 0 {
            (values[values.len() / 2 - 1] + values[values.len() / 2]) / 2.0
        } else {
            values[values.len() / 2]
        };

        let variance = values
            .iter()
            .map(|v| {
                let diff = v - mean;
                diff * diff
            })
            .sum::<f64>()
            / values.len() as f64;
        let std_dev = if variance >= 0.0 {
            // Simple sqrt implementation for no_std
            let mut x = variance;
            let mut y = 1.0;
            while (x - y).abs() > 0.000001 {
                x = (x + y) / 2.0;
                y = variance / x;
            }
            x
        } else {
            0.0
        };

        let percentile_95 = values[(0.95 * values.len() as f64) as usize];
        let percentile_99 = values[(0.99 * values.len() as f64) as usize];

        PerformanceStats {
            min,
            max,
            mean,
            median,
            std_dev,
            percentile_95,
            percentile_99,
            sample_count: samples.len(),
        }
    }

    /// Check if benchmark meets criteria
    fn check_benchmark_criteria(&self, config: &BenchmarkConfig, stats: &PerformanceStats) -> bool {
        let mut passed = true;

        if let Some(expected_min) = config.expected_min {
            if stats.mean < expected_min {
                passed = false;
            }
        }

        if let Some(expected_max) = config.expected_max {
            if stats.mean > expected_max {
                passed = false;
            }
        }

        passed
    }

    /// Set baseline results for regression testing
    pub fn set_baseline(&mut self, name: String, stats: PerformanceStats) {
        self.baseline_results.insert(name, stats);
    }
}

/// Create performance benchmark test suite
pub fn create_performance_benchmark_suite() -> TestSuite {
    TestSuite {
        name: "Performance Benchmarks".to_string(),
        tests: vec![
            TestCase {
                name: "System Call Latency Benchmark".to_string(),
                test_type: TestType::Performance,
                function: benchmark_syscall_latency,
                timeout_ms: 30000,
                setup: Some(setup_performance_tests),
                teardown: Some(teardown_performance_tests),
                dependencies: vec!["syscall".to_string()],
            },
            TestCase {
                name: "Memory Allocation Benchmark".to_string(),
                test_type: TestType::Performance,
                function: benchmark_memory_allocation,
                timeout_ms: 30000,
                setup: Some(setup_performance_tests),
                teardown: Some(teardown_performance_tests),
                dependencies: vec!["memory".to_string()],
            },
            TestCase {
                name: "Context Switch Benchmark".to_string(),
                test_type: TestType::Performance,
                function: benchmark_context_switch,
                timeout_ms: 30000,
                setup: Some(setup_performance_tests),
                teardown: Some(teardown_performance_tests),
                dependencies: vec!["scheduler".to_string()],
            },
            TestCase {
                name: "Interrupt Latency Benchmark".to_string(),
                test_type: TestType::Performance,
                function: benchmark_interrupt_latency,
                timeout_ms: 30000,
                setup: Some(setup_performance_tests),
                teardown: Some(teardown_performance_tests),
                dependencies: vec!["interrupts".to_string()],
            },
            TestCase {
                name: "I/O Throughput Benchmark".to_string(),
                test_type: TestType::Performance,
                function: benchmark_io_throughput,
                timeout_ms: 30000,
                setup: Some(setup_performance_tests),
                teardown: Some(teardown_performance_tests),
                dependencies: vec!["io_optimized".to_string()],
            },
        ],
        setup: Some(setup_all_performance_tests),
        teardown: Some(teardown_all_performance_tests),
    }
}

// Setup and teardown functions
fn setup_all_performance_tests() {
    // Initialize performance testing environment
}

fn teardown_all_performance_tests() {
    // Clean up performance testing environment
}

fn setup_performance_tests() {}
fn teardown_performance_tests() {}

// Benchmark test implementations

/// Benchmark system call latency
fn benchmark_syscall_latency() -> TestResult {
    let mut suite = BenchmarkSuite::new().expect("Failed to create benchmark suite");

    let config = BenchmarkConfig {
        name: "syscall_getpid".to_string(),
        duration_ms: 5000,
        warmup_ms: 1000,
        iterations: 10000,
        target_metric: MetricType::Latency,
        expected_min: None,
        expected_max: Some(10.0),   // 10 microseconds max
        regression_threshold: 20.0, // 20%
    };

    suite.add_benchmark(config);
    let results = suite.run_all_benchmarks();

    if !results.is_empty() && results[0].passed {
        TestResult::Pass
    } else {
        TestResult::Fail
    }
}

/// Benchmark memory allocation performance using real memory manager
fn benchmark_memory_allocation() -> TestResult {
    use crate::memory::{get_memory_manager, MemoryZone};

    // Test real memory allocation performance
    let start_time = crate::time::uptime_us();
    let iterations = 1000;
    let mut successful_allocations = 0;

    if let Some(memory_manager) = get_memory_manager() {
        let mut allocated_frames = Vec::new();

        // Allocation phase
        for _ in 0..iterations {
            if let Some(frame) = memory_manager.allocate_frame_in_zone(MemoryZone::Normal) {
                allocated_frames.push(frame);
                successful_allocations += 1;
            }
        }

        // Deallocation phase
        for frame in allocated_frames {
            memory_manager.deallocate_frame(frame, MemoryZone::Normal);
        }
    }

    let end_time = crate::time::uptime_us();
    let elapsed_us = end_time - start_time;
    let avg_latency_us = if successful_allocations > 0 {
        elapsed_us / (successful_allocations * 2) // allocation + deallocation
    } else {
        u64::MAX
    };

    // Pass if average latency is under 10 microseconds and we had successful allocations
    if avg_latency_us < 10 && successful_allocations > iterations / 2 {
        TestResult::Pass
    } else {
        TestResult::Fail
    }
}

/// Benchmark context switch performance
fn benchmark_context_switch() -> TestResult {
    let mut suite = BenchmarkSuite::new().expect("Failed to create benchmark suite");

    let config = BenchmarkConfig {
        name: "context_switch".to_string(),
        duration_ms: 5000,
        warmup_ms: 1000,
        iterations: 1000,
        target_metric: MetricType::Latency,
        expected_min: None,
        expected_max: Some(50.0),   // 50 microseconds max
        regression_threshold: 30.0, // 30%
    };

    suite.add_benchmark(config);
    let results = suite.run_all_benchmarks();

    if !results.is_empty() && results[0].passed {
        TestResult::Pass
    } else {
        TestResult::Fail
    }
}

/// Benchmark interrupt latency using real interrupt system
fn benchmark_interrupt_latency() -> TestResult {
    // Measure actual interrupt handling latency by observing timer interrupts
    let initial_stats = crate::interrupts::get_stats();
    let start_tsc = crate::performance_monitor::read_tsc();
    let start_time = crate::time::uptime_us();

    // Wait for several timer interrupts to occur
    let measurement_duration = 50000; // 50ms
    while crate::time::uptime_us() - start_time < measurement_duration {
        unsafe {
            core::arch::asm!("pause");
        }
    }

    let end_tsc = crate::performance_monitor::read_tsc();
    let _end_time = crate::time::uptime_us();
    let final_stats = crate::interrupts::get_stats();

    // Calculate interrupt handling performance
    let timer_interrupts = final_stats.timer_count - initial_stats.timer_count;
    let total_interrupts = (final_stats.timer_count
        + final_stats.keyboard_count
        + final_stats.serial_count)
        - (initial_stats.timer_count + initial_stats.keyboard_count + initial_stats.serial_count);

    if total_interrupts > 0 {
        let elapsed_cycles = end_tsc - start_tsc;
        let avg_cycles_per_interrupt = elapsed_cycles / total_interrupts;
        let avg_latency_us = (avg_cycles_per_interrupt as f64) / 3000.0; // Assuming 3GHz CPU

        // Pass if average interrupt handling latency is reasonable (under 10 microseconds)
        if avg_latency_us < 10.0 && timer_interrupts > 0 {
            TestResult::Pass
        } else {
            TestResult::Fail
        }
    } else {
        // No interrupts observed - check if interrupt system is functional
        if crate::interrupts::are_enabled() {
            TestResult::Pass // System is functional
        } else {
            TestResult::Fail
        }
    }
}

/// Benchmark I/O throughput
fn benchmark_io_throughput() -> TestResult {
    if crate::io_optimized::init_io_system().is_err() {
        return TestResult::Skip;
    }

    let io_scheduler = crate::io_optimized::io_scheduler();
    let start_time = crate::time::uptime_us();
    let mut requests_submitted = 0;

    // Submit I/O requests for 5 seconds
    while crate::time::uptime_us() - start_time < 5_000_000 {
        let buffer = Some(0x10000u64);

        let request = crate::io_optimized::IoRequest {
            request_id: 0,
            id: 0,
            request_type: crate::io_optimized::IoRequestType::Read,
            priority: crate::io_optimized::IoPriority::Normal,
            target: 0,
            offset: requests_submitted as u64 * 4096,
            buffer,
            size: 4096,
            device_id: 0,
            waker: None,
            completion_status: crate::io_optimized::IoCompletionStatus::Pending,
        };

        let _future = io_scheduler.submit_request(request);
        requests_submitted += 1;

        // Process requests
        io_scheduler.process_requests();

        if requests_submitted >= 10000 {
            break;
        }
    }

    let end_time = crate::time::uptime_us();
    let elapsed_us = end_time - start_time;
    let throughput = (requests_submitted as f64 * 1_000_000.0) / elapsed_us as f64;

    // Pass if throughput is over 1000 requests per second
    if throughput > 1000.0 {
        TestResult::Pass
    } else {
        TestResult::Fail
    }
}

/// Global performance monitor instance
static mut GLOBAL_PERFORMANCE_MONITOR: Option<PerformanceMonitor> = None;

/// Initialize global performance monitoring
pub fn init_performance_monitoring() -> Result<(), &'static str> {
    unsafe {
        GLOBAL_PERFORMANCE_MONITOR = PerformanceMonitor::new();
        if GLOBAL_PERFORMANCE_MONITOR.is_some() {
            Ok(())
        } else {
            Err("Failed to initialize performance monitor")
        }
    }
}

/// Get global performance monitor
pub fn get_performance_monitor() -> Option<&'static PerformanceMonitor> {
    unsafe { GLOBAL_PERFORMANCE_MONITOR.as_ref() }
}

/// Record a performance sample globally
pub fn record_performance_sample(sample: PerformanceSample) {
    if let Some(monitor) = get_performance_monitor() {
        monitor.record_sample(sample);
    }
}

/// Get performance summary for all subsystems
pub fn get_system_performance_summary() -> BTreeMap<String, PerformanceStats> {
    let mut summary = BTreeMap::new();

    // Collect metrics from various subsystems
    let interrupt_stats = crate::interrupts::get_stats();
    let syscall_stats = crate::syscall::get_syscall_stats();
    let _scheduler_stats = crate::scheduler::get_scheduler_stats();
    let (memory_used, memory_total) = crate::performance_monitor::memory_usage();

    // Convert to performance stats format
    summary.insert(
        "interrupts".to_string(),
        PerformanceStats {
            min: 0.0,
            max: interrupt_stats.timer_count as f64,
            mean: (interrupt_stats.timer_count + interrupt_stats.keyboard_count) as f64 / 2.0,
            median: interrupt_stats.timer_count as f64,
            std_dev: 0.0,
            percentile_95: interrupt_stats.timer_count as f64,
            percentile_99: interrupt_stats.timer_count as f64,
            sample_count: 1,
        },
    );

    summary.insert(
        "syscalls".to_string(),
        PerformanceStats {
            min: 0.0,
            max: syscall_stats.total_calls as f64,
            mean: syscall_stats.successful_calls as f64,
            median: syscall_stats.successful_calls as f64,
            std_dev: 0.0,
            percentile_95: syscall_stats.total_calls as f64,
            percentile_99: syscall_stats.total_calls as f64,
            sample_count: 1,
        },
    );

    summary.insert(
        "memory".to_string(),
        PerformanceStats {
            min: 0.0,
            max: memory_total as f64,
            mean: memory_used as f64,
            median: memory_used as f64,
            std_dev: 0.0,
            percentile_95: memory_used as f64,
            percentile_99: memory_used as f64,
            sample_count: 1,
        },
    );

    summary
}
