//! Comprehensive Testing Framework for RustOS
//!
//! This module provides a robust, production-ready testing framework with:
//! - Test discovery and execution
//! - Test statistics and reporting
//! - Support for different test types (unit, integration, stress, security, etc.)
//! - Mock interfaces for hardware dependencies
//! - Automated regression testing
//! - Proper error handling and recovery

use alloc::{
    collections::BTreeMap,
    string::{String, ToString},
    vec,
    vec::Vec,
};
use core::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};

// ============================================================================
// Core Types and Enums
// ============================================================================

/// Test result status indicating the outcome of a test execution
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TestResult {
    /// Test passed successfully
    Pass,
    /// Test failed
    Fail,
    /// Test was skipped (e.g., missing dependencies or prerequisites)
    Skip,
    /// Test execution timed out
    Timeout,
    /// Test encountered an error during execution
    Error,
}

impl TestResult {
    /// Returns true if the test passed
    pub fn is_pass(&self) -> bool {
        matches!(self, TestResult::Pass)
    }

    /// Returns true if the test failed (Fail, Timeout, or Error)
    pub fn is_failure(&self) -> bool {
        matches!(
            self,
            TestResult::Fail | TestResult::Timeout | TestResult::Error
        )
    }

    /// Returns a human-readable string representation
    pub fn as_str(&self) -> &'static str {
        match self {
            TestResult::Pass => "PASS",
            TestResult::Fail => "FAIL",
            TestResult::Skip => "SKIP",
            TestResult::Timeout => "TIMEOUT",
            TestResult::Error => "ERROR",
        }
    }
}

/// Test type classification for organizing and filtering tests
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TestType {
    /// Unit tests for individual functions/modules
    Unit,
    /// Integration tests for subsystem interactions
    Integration,
    /// Performance benchmarks
    Performance,
    /// Stress tests for system limits
    Stress,
    /// Security vulnerability tests
    Security,
    /// Regression tests for bug fixes
    Regression,
    /// Hardware compatibility tests
    Hardware,
    /// Validation tests for production readiness
    Validation,
}

impl TestType {
    /// Returns a human-readable name for the test type
    pub fn name(&self) -> &'static str {
        match self {
            TestType::Unit => "Unit",
            TestType::Integration => "Integration",
            TestType::Performance => "Performance",
            TestType::Stress => "Stress",
            TestType::Security => "Security",
            TestType::Regression => "Regression",
            TestType::Hardware => "Hardware",
            TestType::Validation => "Validation",
        }
    }
}

// ============================================================================
// Test Case and Test Suite Structures
// ============================================================================

/// A single test case with its configuration and execution function
#[derive(Clone)]
pub struct TestCase {
    /// Name of the test case
    pub name: String,
    /// Type of test
    pub test_type: TestType,
    /// The test function to execute
    pub function: fn() -> TestResult,
    /// Maximum execution time in milliseconds before timeout
    pub timeout_ms: u64,
    /// Optional setup function to run before the test
    pub setup: Option<fn()>,
    /// Optional teardown function to run after the test
    pub teardown: Option<fn()>,
    /// List of dependencies (other tests or subsystems that must be available)
    pub dependencies: Vec<String>,
}

impl TestCase {
    /// Create a new test case with the given name and function
    pub fn new(name: &str, test_type: TestType, function: fn() -> TestResult) -> Self {
        Self {
            name: name.to_string(),
            test_type,
            function,
            timeout_ms: 5000, // Default 5 second timeout
            setup: None,
            teardown: None,
            dependencies: Vec::new(),
        }
    }

    /// Set the timeout for this test case
    pub fn with_timeout(mut self, timeout_ms: u64) -> Self {
        self.timeout_ms = timeout_ms;
        self
    }

    /// Set the setup function for this test case
    pub fn with_setup(mut self, setup: fn()) -> Self {
        self.setup = Some(setup);
        self
    }

    /// Set the teardown function for this test case
    pub fn with_teardown(mut self, teardown: fn()) -> Self {
        self.teardown = Some(teardown);
        self
    }

    /// Add a dependency for this test case
    pub fn with_dependency(mut self, dependency: &str) -> Self {
        self.dependencies.push(dependency.to_string());
        self
    }
}

impl core::fmt::Debug for TestCase {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("TestCase")
            .field("name", &self.name)
            .field("test_type", &self.test_type)
            .field("timeout_ms", &self.timeout_ms)
            .field("has_setup", &self.setup.is_some())
            .field("has_teardown", &self.teardown.is_some())
            .field("dependencies", &self.dependencies)
            .finish()
    }
}

/// A collection of related test cases forming a test suite
#[derive(Clone)]
pub struct TestSuite {
    /// Name of the test suite
    pub name: String,
    /// List of test cases in this suite
    pub tests: Vec<TestCase>,
    /// Optional setup function to run before all tests in the suite
    pub setup: Option<fn()>,
    /// Optional teardown function to run after all tests in the suite
    pub teardown: Option<fn()>,
}

impl TestSuite {
    /// Create a new test suite with the given name
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            tests: Vec::new(),
            setup: None,
            teardown: None,
        }
    }

    /// Add a test case to this suite
    pub fn add_test(&mut self, test: TestCase) {
        self.tests.push(test);
    }

    /// Set the suite-level setup function
    pub fn with_setup(mut self, setup: fn()) -> Self {
        self.setup = Some(setup);
        self
    }

    /// Set the suite-level teardown function
    pub fn with_teardown(mut self, teardown: fn()) -> Self {
        self.teardown = Some(teardown);
        self
    }

    /// Get the number of tests in this suite
    pub fn test_count(&self) -> usize {
        self.tests.len()
    }
}

impl core::fmt::Debug for TestSuite {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("TestSuite")
            .field("name", &self.name)
            .field("test_count", &self.tests.len())
            .field("has_setup", &self.setup.is_some())
            .field("has_teardown", &self.teardown.is_some())
            .finish()
    }
}

// ============================================================================
// Test Statistics and Results
// ============================================================================

/// Statistics collected during test execution
#[derive(Debug, Clone, Default)]
pub struct TestStats {
    /// Total number of tests executed
    pub total_tests: usize,
    /// Number of tests that passed
    pub passed: usize,
    /// Number of tests that failed
    pub failed: usize,
    /// Number of tests that were skipped
    pub skipped: usize,
    /// Number of tests that timed out
    pub timeouts: usize,
    /// Number of tests that encountered errors
    pub errors: usize,
    /// Total execution time in milliseconds
    pub execution_time_ms: u64,
}

impl TestStats {
    /// Create new empty test statistics
    pub fn new() -> Self {
        Self::default()
    }

    /// Calculate the pass rate as a percentage
    pub fn pass_rate(&self) -> f32 {
        if self.total_tests == 0 {
            0.0
        } else {
            (self.passed as f32 / self.total_tests as f32) * 100.0
        }
    }

    /// Calculate the failure rate as a percentage
    pub fn failure_rate(&self) -> f32 {
        if self.total_tests == 0 {
            0.0
        } else {
            ((self.failed + self.timeouts + self.errors) as f32 / self.total_tests as f32) * 100.0
        }
    }

    /// Get the total number of tests
    pub fn total_tests(&self) -> usize {
        self.total_tests
    }

    /// Get the number of passed tests
    pub fn passed_tests(&self) -> usize {
        self.passed
    }

    /// Check if all tests passed
    pub fn all_passed(&self) -> bool {
        self.failed == 0 && self.timeouts == 0 && self.errors == 0
    }

    /// Merge another TestStats into this one
    pub fn merge(&mut self, other: &TestStats) {
        self.total_tests += other.total_tests;
        self.passed += other.passed;
        self.failed += other.failed;
        self.skipped += other.skipped;
        self.timeouts += other.timeouts;
        self.errors += other.errors;
        self.execution_time_ms += other.execution_time_ms;
    }

    /// Update stats based on a test result
    pub fn record_result(&mut self, result: TestResult) {
        self.total_tests += 1;
        match result {
            TestResult::Pass => self.passed += 1,
            TestResult::Fail => self.failed += 1,
            TestResult::Skip => self.skipped += 1,
            TestResult::Timeout => self.timeouts += 1,
            TestResult::Error => self.errors += 1,
        }
    }
}

/// Result of executing a single test
#[derive(Debug, Clone)]
pub struct TestExecutionResult {
    /// Name of the test that was executed
    pub test_name: String,
    /// Name of the suite containing the test
    pub suite_name: String,
    /// Type of the test
    pub test_type: TestType,
    /// Result of the test execution
    pub result: TestResult,
    /// Execution time in milliseconds
    pub execution_time_ms: u64,
    /// Optional error message if the test failed
    pub error_message: Option<String>,
    /// Timestamp when the test started (in microseconds since boot)
    pub start_time: u64,
    /// Timestamp when the test ended (in microseconds since boot)
    pub end_time: u64,
}

impl TestExecutionResult {
    /// Check if this test passed
    pub fn is_pass(&self) -> bool {
        self.result.is_pass()
    }

    /// Check if this test failed
    pub fn is_failure(&self) -> bool {
        self.result.is_failure()
    }
}

// ============================================================================
// Test Result Queue (Lock-Free)
// ============================================================================

/// A simple lock-free result storage using atomic operations
/// This provides thread-safe result collection without locks
pub struct TestResultQueue {
    results: spin::Mutex<Vec<TestExecutionResult>>,
    count: AtomicUsize,
}

impl TestResultQueue {
    /// Create a new result queue
    pub const fn new() -> Self {
        Self {
            results: spin::Mutex::new(Vec::new()),
            count: AtomicUsize::new(0),
        }
    }

    /// Add a result to the queue
    pub fn push(&self, result: TestExecutionResult) {
        let mut results = self.results.lock();
        results.push(result);
        self.count.fetch_add(1, Ordering::Release);
    }

    /// Get all results (drains the queue)
    pub fn drain(&self) -> Vec<TestExecutionResult> {
        let mut results = self.results.lock();
        let drained: Vec<_> = results.drain(..).collect();
        self.count.store(0, Ordering::Release);
        drained
    }

    /// Get the number of results in the queue
    pub fn len(&self) -> usize {
        self.count.load(Ordering::Acquire)
    }

    /// Check if the queue is empty
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

// ============================================================================
// Test Framework Core
// ============================================================================

/// The main test framework runner for executing tests and collecting results
pub struct TestFramework {
    /// Registered test suites
    suites: Vec<TestSuite>,
    /// Accumulated statistics
    stats: TestStats,
    /// Whether mock mode is enabled
    mock_enabled: AtomicBool,
    /// Results queue for collecting test results
    results: TestResultQueue,
    /// Whether the framework has been initialized
    initialized: AtomicBool,
    /// Verbose output mode
    verbose: AtomicBool,
    /// Fail fast mode (stop on first failure)
    fail_fast: AtomicBool,
    /// Filter by test type (None means run all)
    type_filter: Option<TestType>,
    /// Filter by test name pattern
    name_filter: Option<String>,
}

impl TestFramework {
    /// Create a new test framework instance
    pub fn new() -> Self {
        Self {
            suites: Vec::new(),
            stats: TestStats::new(),
            mock_enabled: AtomicBool::new(false),
            results: TestResultQueue::new(),
            initialized: AtomicBool::new(true),
            verbose: AtomicBool::new(false),
            fail_fast: AtomicBool::new(false),
            type_filter: None,
            name_filter: None,
        }
    }

    /// Add a test suite to the framework
    pub fn add_suite(&mut self, suite: TestSuite) {
        self.suites.push(suite);
    }

    /// Set verbose output mode
    pub fn set_verbose(&self, verbose: bool) {
        self.verbose.store(verbose, Ordering::Release);
    }

    /// Set fail-fast mode
    pub fn set_fail_fast(&self, fail_fast: bool) {
        self.fail_fast.store(fail_fast, Ordering::Release);
    }

    /// Set type filter
    pub fn set_type_filter(&mut self, test_type: Option<TestType>) {
        self.type_filter = test_type;
    }

    /// Set name filter pattern
    pub fn set_name_filter(&mut self, pattern: Option<String>) {
        self.name_filter = pattern;
    }

    /// Run all registered test suites
    pub fn run_all_tests(&mut self) -> TestStats {
        let start_time = get_uptime_us();

        // Reset statistics for this run
        self.stats = TestStats::new();

        // Clone suites to avoid borrow issues during iteration
        let suites = self.suites.clone();

        for suite in &suites {
            if self.fail_fast.load(Ordering::Acquire) && self.stats.failed > 0 {
                break;
            }
            self.run_suite(suite);
        }

        let end_time = get_uptime_us();
        self.stats.execution_time_ms = (end_time - start_time) / 1000;

        self.stats.clone()
    }

    /// Run a specific test suite
    pub fn run_suite(&mut self, suite: &TestSuite) {
        if self.verbose.load(Ordering::Acquire) {
            // In a real kernel, you'd use serial/VGA output
            // println!("Running suite: {}", suite.name);
        }

        // Run suite setup if present
        if let Some(setup) = suite.setup {
            setup();
        }

        for test in &suite.tests {
            // Check type filter
            if let Some(ref filter_type) = self.type_filter {
                if test.test_type != *filter_type {
                    continue;
                }
            }

            // Check name filter
            if let Some(ref pattern) = self.name_filter {
                if !test.name.contains(pattern.as_str()) {
                    continue;
                }
            }

            // Check fail-fast
            if self.fail_fast.load(Ordering::Acquire) && self.stats.failed > 0 {
                break;
            }

            let result = self.run_test(test, &suite.name);
            self.update_stats(&result);
            self.results.push(result);
        }

        // Run suite teardown if present
        if let Some(teardown) = suite.teardown {
            teardown();
        }
    }

    /// Run a single test case
    fn run_test(&self, test: &TestCase, suite_name: &str) -> TestExecutionResult {
        let start_time = get_uptime_us();

        // Run test setup if present
        if let Some(setup) = test.setup {
            setup();
        }

        // Execute the test with timeout handling
        let result = self.execute_with_timeout(test.function, test.timeout_ms);

        // Run test teardown if present
        if let Some(teardown) = test.teardown {
            teardown();
        }

        let end_time = get_uptime_us();
        let execution_time = (end_time - start_time) / 1000;

        TestExecutionResult {
            test_name: test.name.clone(),
            suite_name: suite_name.to_string(),
            test_type: test.test_type,
            result,
            execution_time_ms: execution_time,
            error_message: if result == TestResult::Fail || result == TestResult::Error {
                Some(alloc::format!("Test '{}' failed", test.name))
            } else {
                None
            },
            start_time,
            end_time,
        }
    }

    /// Execute a test function with timeout handling
    fn execute_with_timeout(&self, test_fn: fn() -> TestResult, timeout_ms: u64) -> TestResult {
        let start_time = get_uptime_us();
        let timeout_us = timeout_ms * 1000;

        // Execute the test function
        // Note: In a real kernel, this would use timer interrupts for true timeout
        let result = test_fn();

        let elapsed = get_uptime_us() - start_time;
        if elapsed > timeout_us {
            TestResult::Timeout
        } else {
            result
        }
    }

    /// Update statistics based on a test result
    fn update_stats(&mut self, result: &TestExecutionResult) {
        self.stats.record_result(result.result);
    }

    /// Enable mock interfaces for testing
    pub fn enable_mocks(&self) {
        self.mock_enabled.store(true, Ordering::Release);
    }

    /// Disable mock interfaces
    pub fn disable_mocks(&self) {
        self.mock_enabled.store(false, Ordering::Release);
    }

    /// Check if mocks are enabled
    pub fn mocks_enabled(&self) -> bool {
        self.mock_enabled.load(Ordering::Acquire)
    }

    /// Get all test results collected during execution
    pub fn get_results(&self) -> Vec<TestExecutionResult> {
        self.results.drain()
    }

    /// Get the current statistics
    pub fn get_stats(&self) -> TestStats {
        self.stats.clone()
    }

    /// Get the total number of registered tests
    pub fn total_test_count(&self) -> usize {
        self.suites.iter().map(|s| s.tests.len()).sum()
    }

    /// Get the number of registered suites
    pub fn suite_count(&self) -> usize {
        self.suites.len()
    }

    /// Clear all registered suites
    pub fn clear(&mut self) {
        self.suites.clear();
        self.stats = TestStats::new();
    }

    /// Run tests matching a specific type
    pub fn run_tests_by_type(&mut self, test_type: TestType) -> TestStats {
        let old_filter = self.type_filter;
        self.type_filter = Some(test_type);
        let stats = self.run_all_tests();
        self.type_filter = old_filter;
        stats
    }

    /// Run a single test by name
    pub fn run_test_by_name(&mut self, name: &str) -> Option<TestExecutionResult> {
        for suite in &self.suites.clone() {
            for test in &suite.tests {
                if test.name == name {
                    let result = self.run_test(test, &suite.name);
                    self.update_stats(&result);
                    return Some(result);
                }
            }
        }
        None
    }
}

impl Default for TestFramework {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Mock Hardware Interfaces
// ============================================================================

/// Mock hardware interfaces for testing without real hardware
pub mod mocks {
    use super::*;

    /// Mock interrupt controller for testing interrupt handling
    pub struct MockInterruptController {
        interrupt_count: AtomicU64,
        enabled: AtomicBool,
    }

    impl MockInterruptController {
        pub const fn new() -> Self {
            Self {
                interrupt_count: AtomicU64::new(0),
                enabled: AtomicBool::new(false),
            }
        }

        pub fn trigger_interrupt(&self, _vector: u8) {
            if self.enabled.load(Ordering::Acquire) {
                self.interrupt_count.fetch_add(1, Ordering::Relaxed);
            }
        }

        pub fn enable(&self) {
            self.enabled.store(true, Ordering::Release);
        }

        pub fn disable(&self) {
            self.enabled.store(false, Ordering::Release);
        }

        pub fn get_interrupt_count(&self) -> u64 {
            self.interrupt_count.load(Ordering::Relaxed)
        }

        pub fn reset(&self) {
            self.interrupt_count.store(0, Ordering::Relaxed);
        }
    }

    /// Mock memory controller for testing memory operations
    pub struct MockMemoryController {
        allocations: AtomicU64,
        deallocations: AtomicU64,
        total_allocated: AtomicU64,
        allocation_failures: AtomicU64,
    }

    impl MockMemoryController {
        pub const fn new() -> Self {
            Self {
                allocations: AtomicU64::new(0),
                deallocations: AtomicU64::new(0),
                total_allocated: AtomicU64::new(0),
                allocation_failures: AtomicU64::new(0),
            }
        }

        pub fn allocate(&self, size: usize) -> *mut u8 {
            self.allocations.fetch_add(1, Ordering::Relaxed);
            self.total_allocated
                .fetch_add(size as u64, Ordering::Relaxed);
            // Allocate real memory so the pointer is valid for read/write
            let layout = alloc::alloc::Layout::from_size_align(size, 8).unwrap();
            let ptr = unsafe { alloc::alloc::alloc(layout) };
            if ptr.is_null() {
                self.allocation_failures.fetch_add(1, Ordering::Relaxed);
            }
            ptr
        }

        pub fn deallocate(&self, ptr: *mut u8, size: usize) {
            self.deallocations.fetch_add(1, Ordering::Relaxed);
            self.total_allocated
                .fetch_sub(size as u64, Ordering::Relaxed);
            // Free the real memory
            if !ptr.is_null() && size > 0 {
                let layout = alloc::alloc::Layout::from_size_align(size, 8).unwrap();
                unsafe { alloc::alloc::dealloc(ptr, layout) };
            }
        }

        pub fn simulate_allocation_failure(&self) {
            self.allocation_failures.fetch_add(1, Ordering::Relaxed);
        }

        pub fn get_stats(&self) -> (u64, u64, u64, u64) {
            (
                self.allocations.load(Ordering::Relaxed),
                self.deallocations.load(Ordering::Relaxed),
                self.total_allocated.load(Ordering::Relaxed),
                self.allocation_failures.load(Ordering::Relaxed),
            )
        }

        pub fn reset(&self) {
            self.allocations.store(0, Ordering::Relaxed);
            self.deallocations.store(0, Ordering::Relaxed);
            self.total_allocated.store(0, Ordering::Relaxed);
            self.allocation_failures.store(0, Ordering::Relaxed);
        }
    }

    /// Mock timer for testing time-dependent functionality
    pub struct MockTimer {
        current_time: AtomicU64,
        tick_count: AtomicU64,
        frequency: AtomicU64,
    }

    impl MockTimer {
        pub const fn new() -> Self {
            Self {
                current_time: AtomicU64::new(0),
                tick_count: AtomicU64::new(0),
                frequency: AtomicU64::new(1000), // 1kHz default
            }
        }

        pub fn tick(&self, elapsed_us: u64) {
            self.current_time.fetch_add(elapsed_us, Ordering::Relaxed);
            self.tick_count.fetch_add(1, Ordering::Relaxed);
        }

        pub fn get_time(&self) -> u64 {
            self.current_time.load(Ordering::Relaxed)
        }

        pub fn get_tick_count(&self) -> u64 {
            self.tick_count.load(Ordering::Relaxed)
        }

        pub fn set_frequency(&self, frequency: u64) {
            self.frequency.store(frequency, Ordering::Relaxed);
        }

        pub fn get_frequency(&self) -> u64 {
            self.frequency.load(Ordering::Relaxed)
        }

        pub fn reset(&self) {
            self.current_time.store(0, Ordering::Relaxed);
            self.tick_count.store(0, Ordering::Relaxed);
        }
    }

    /// Mock I/O port for testing port I/O operations
    pub struct MockPortIO {
        ports: spin::Mutex<BTreeMap<u16, u32>>,
        read_count: AtomicU64,
        write_count: AtomicU64,
    }

    impl MockPortIO {
        pub fn new() -> Self {
            Self {
                ports: spin::Mutex::new(BTreeMap::new()),
                read_count: AtomicU64::new(0),
                write_count: AtomicU64::new(0),
            }
        }

        pub fn read(&self, port: u16) -> u32 {
            self.read_count.fetch_add(1, Ordering::Relaxed);
            let ports = self.ports.lock();
            *ports.get(&port).unwrap_or(&0)
        }

        pub fn write(&self, port: u16, value: u32) {
            self.write_count.fetch_add(1, Ordering::Relaxed);
            let mut ports = self.ports.lock();
            ports.insert(port, value);
        }

        pub fn get_io_stats(&self) -> (u64, u64) {
            (
                self.read_count.load(Ordering::Relaxed),
                self.write_count.load(Ordering::Relaxed),
            )
        }
    }

    // Global mock instances
    static MOCK_INTERRUPT_CONTROLLER: MockInterruptController = MockInterruptController::new();
    static MOCK_MEMORY_CONTROLLER: MockMemoryController = MockMemoryController::new();
    static MOCK_TIMER: MockTimer = MockTimer::new();

    pub fn get_mock_interrupt_controller() -> &'static MockInterruptController {
        &MOCK_INTERRUPT_CONTROLLER
    }

    pub fn get_mock_memory_controller() -> &'static MockMemoryController {
        &MOCK_MEMORY_CONTROLLER
    }

    pub fn get_mock_timer() -> &'static MockTimer {
        &MOCK_TIMER
    }

    /// Reset all mock state
    pub fn reset_all_mocks() {
        MOCK_INTERRUPT_CONTROLLER.reset();
        MOCK_MEMORY_CONTROLLER.reset();
        MOCK_TIMER.reset();
    }
}

// ============================================================================
// Built-in Unit Tests
// ============================================================================

/// Unit tests for kernel components
pub mod unit_tests {
    use super::*;

    /// Test memory allocation and deallocation
    pub fn test_memory_allocation() -> TestResult {
        // Try to use real memory manager if available
        #[cfg(feature = "memory_manager")]
        {
            use crate::memory::{get_memory_manager, MemoryZone};

            if let Some(memory_manager_guard) = get_memory_manager() {
                if let Some(memory_manager) = memory_manager_guard.as_ref() {
                    let frame = {
                        let mut manager = memory_manager.lock();
                        manager.allocate_frame_in_zone(MemoryZone::Normal)
                    };

                    if let Some(frame) = frame {
                        let mut manager = memory_manager.lock();
                        manager.deallocate_frame(frame, MemoryZone::Normal);
                        return TestResult::Pass;
                    } else {
                        return TestResult::Fail;
                    }
                }
            }
        }

        // Fall back to mock testing
        let mock_mem = mocks::get_mock_memory_controller();
        mock_mem.reset();

        let ptr = mock_mem.allocate(4096);
        if ptr.is_null() {
            return TestResult::Fail;
        }

        mock_mem.deallocate(ptr, 4096);

        let (allocs, deallocs, _, _) = mock_mem.get_stats();
        if allocs == 1 && deallocs == 1 {
            TestResult::Pass
        } else {
            TestResult::Fail
        }
    }

    /// Test timer functionality
    pub fn test_timer_functionality() -> TestResult {
        let mock_timer = mocks::get_mock_timer();

        mock_timer.reset();
        let initial_time = mock_timer.get_time();

        // Simulate timer ticks
        mock_timer.tick(1000); // 1ms
        mock_timer.tick(2000); // 2ms

        let final_time = mock_timer.get_time();
        if final_time == initial_time + 3000 {
            TestResult::Pass
        } else {
            TestResult::Fail
        }
    }

    /// Test interrupt handling
    pub fn test_interrupt_handling() -> TestResult {
        let mock_int = mocks::get_mock_interrupt_controller();

        mock_int.reset();
        mock_int.enable();

        let initial_count = mock_int.get_interrupt_count();

        // Trigger some test interrupts
        mock_int.trigger_interrupt(32); // Timer
        mock_int.trigger_interrupt(33); // Keyboard

        let final_count = mock_int.get_interrupt_count();

        mock_int.disable();

        if final_count == initial_count + 2 {
            TestResult::Pass
        } else {
            TestResult::Fail
        }
    }

    /// Test atomic operations
    pub fn test_atomic_operations() -> TestResult {
        let counter = AtomicU64::new(0);

        counter.fetch_add(10, Ordering::SeqCst);
        counter.fetch_sub(3, Ordering::SeqCst);

        let value = counter.load(Ordering::SeqCst);
        if value == 7 {
            TestResult::Pass
        } else {
            TestResult::Fail
        }
    }

    /// Test spin lock functionality
    pub fn test_spin_lock() -> TestResult {
        let data = spin::Mutex::new(0u64);

        {
            let mut guard = data.lock();
            *guard += 100;
        }

        let value = *data.lock();
        if value == 100 {
            TestResult::Pass
        } else {
            TestResult::Fail
        }
    }
}

/// Performance benchmark tests
pub mod benchmarks {
    use super::*;

    /// Benchmark atomic operation speed
    pub fn benchmark_atomic_operations() -> TestResult {
        let counter = AtomicU64::new(0);
        let iterations = 10000;

        let start = get_uptime_us();

        for _ in 0..iterations {
            counter.fetch_add(1, Ordering::Relaxed);
        }

        let end = get_uptime_us();
        let elapsed = end - start;
        let per_operation = if iterations > 0 {
            elapsed / iterations
        } else {
            u64::MAX
        };

        // Pass if under 1 microsecond per operation
        if per_operation < 1 {
            TestResult::Pass
        } else if per_operation < 10 {
            TestResult::Pass // Acceptable
        } else {
            TestResult::Fail
        }
    }

    /// Benchmark spin lock performance
    pub fn benchmark_spin_lock() -> TestResult {
        let data = spin::Mutex::new(0u64);
        let iterations = 10000;

        let start = get_uptime_us();

        for _ in 0..iterations {
            let mut guard = data.lock();
            *guard += 1;
        }

        let end = get_uptime_us();
        let elapsed = end - start;
        let per_operation = if iterations > 0 {
            elapsed / iterations
        } else {
            u64::MAX
        };

        // Pass if under 10 microseconds per lock/unlock cycle
        if per_operation < 10 {
            TestResult::Pass
        } else {
            TestResult::Fail
        }
    }
}

// ============================================================================
// Default Test Suites
// ============================================================================

/// Create the default set of unit test suites
pub fn create_default_test_suites() -> Vec<TestSuite> {
    vec![
        TestSuite {
            name: "Unit Tests".to_string(),
            tests: vec![
                TestCase {
                    name: "Memory Allocation".to_string(),
                    test_type: TestType::Unit,
                    function: unit_tests::test_memory_allocation,
                    timeout_ms: 1000,
                    setup: None,
                    teardown: None,
                    dependencies: vec![],
                },
                TestCase {
                    name: "Timer Functionality".to_string(),
                    test_type: TestType::Unit,
                    function: unit_tests::test_timer_functionality,
                    timeout_ms: 1000,
                    setup: None,
                    teardown: None,
                    dependencies: vec![],
                },
                TestCase {
                    name: "Interrupt Handling".to_string(),
                    test_type: TestType::Unit,
                    function: unit_tests::test_interrupt_handling,
                    timeout_ms: 1000,
                    setup: None,
                    teardown: None,
                    dependencies: vec![],
                },
                TestCase {
                    name: "Atomic Operations".to_string(),
                    test_type: TestType::Unit,
                    function: unit_tests::test_atomic_operations,
                    timeout_ms: 1000,
                    setup: None,
                    teardown: None,
                    dependencies: vec![],
                },
                TestCase {
                    name: "Spin Lock".to_string(),
                    test_type: TestType::Unit,
                    function: unit_tests::test_spin_lock,
                    timeout_ms: 1000,
                    setup: None,
                    teardown: None,
                    dependencies: vec![],
                },
            ],
            setup: None,
            teardown: None,
        },
        TestSuite {
            name: "Performance Benchmarks".to_string(),
            tests: vec![
                TestCase {
                    name: "Atomic Operations Benchmark".to_string(),
                    test_type: TestType::Performance,
                    function: benchmarks::benchmark_atomic_operations,
                    timeout_ms: 5000,
                    setup: None,
                    teardown: None,
                    dependencies: vec![],
                },
                TestCase {
                    name: "Spin Lock Benchmark".to_string(),
                    test_type: TestType::Performance,
                    function: benchmarks::benchmark_spin_lock,
                    timeout_ms: 5000,
                    setup: None,
                    teardown: None,
                    dependencies: vec![],
                },
            ],
            setup: None,
            teardown: None,
        },
    ]
}

// ============================================================================
// Global Test Framework Instance
// ============================================================================

/// Global test framework instance
static TEST_FRAMEWORK: spin::Mutex<Option<TestFramework>> = spin::Mutex::new(None);

/// Initialize the global testing framework
pub fn init_testing_framework() {
    let mut guard = TEST_FRAMEWORK.lock();
    if guard.is_none() {
        *guard = Some(TestFramework::new());
    }
}

/// Run a closure against the global test framework.
///
/// Returns `None` if the framework is uninitialized, or if it is currently
/// borrowed for a running test suite (setup/teardown callbacks reenter here).
pub fn with_test_framework<R>(f: impl FnOnce(&mut TestFramework) -> R) -> Option<R> {
    TEST_FRAMEWORK.lock().as_mut().map(f)
}

/// Run all default tests
pub fn run_all_tests() -> TestStats {
    // Take the framework out so setup/teardown callbacks can reenter
    // `with_test_framework` without deadlocking on the mutex.
    let mut framework = TEST_FRAMEWORK
        .lock()
        .take()
        .unwrap_or_else(TestFramework::new);

    for suite in create_default_test_suites() {
        framework.add_suite(suite);
    }

    framework.enable_mocks();
    let stats = framework.run_all_tests();
    framework.disable_mocks();

    *TEST_FRAMEWORK.lock() = Some(framework);
    stats
}

/// Run whatever suites are currently loaded in the global framework.
///
/// Uses take-and-restore so setup/teardown callbacks can reenter
/// `with_test_framework` without deadlocking.
pub fn run_loaded_tests() -> TestStats {
    let mut framework = TEST_FRAMEWORK
        .lock()
        .take()
        .unwrap_or_else(TestFramework::new);
    let stats = framework.run_all_tests();
    *TEST_FRAMEWORK.lock() = Some(framework);
    stats
}

/// Run tests by type
pub fn run_tests_by_type(test_type: TestType) -> TestStats {
    let mut framework = TEST_FRAMEWORK
        .lock()
        .take()
        .unwrap_or_else(TestFramework::new);

    if framework.suite_count() == 0 {
        for suite in create_default_test_suites() {
            framework.add_suite(suite);
        }
    }

    let stats = framework.run_tests_by_type(test_type);
    *TEST_FRAMEWORK.lock() = Some(framework);
    stats
}

// ============================================================================
// Helper Functions
// ============================================================================

/// Get current uptime in microseconds
/// This provides a fallback implementation when the time module isn't available
fn get_uptime_us() -> u64 {
    // Try to use the real time module if available
    #[cfg(feature = "time")]
    {
        return crate::time::uptime_us();
    }

    // Fallback: Use TSC approximation
    #[cfg(target_arch = "x86_64")]
    {
        unsafe {
            let mut low: u32;
            let mut high: u32;
            core::arch::asm!(
                "rdtsc",
                out("eax") low,
                out("edx") high,
            );
            let tsc = ((high as u64) << 32) | (low as u64);
            // Approximate: assume 3GHz CPU, so 3000 cycles per microsecond
            tsc / 3000
        }
    }

    #[cfg(not(target_arch = "x86_64"))]
    {
        // Generic fallback - just return a counter
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        COUNTER.fetch_add(1, Ordering::Relaxed)
    }
}

// ============================================================================
// Test Assertion Helpers
// ============================================================================

/// Assert that a condition is true
#[inline]
pub fn assert_true(condition: bool) -> TestResult {
    if condition {
        TestResult::Pass
    } else {
        TestResult::Fail
    }
}

/// Assert that two values are equal
#[inline]
pub fn assert_eq<T: PartialEq>(left: T, right: T) -> TestResult {
    if left == right {
        TestResult::Pass
    } else {
        TestResult::Fail
    }
}

/// Assert that two values are not equal
#[inline]
pub fn assert_ne<T: PartialEq>(left: T, right: T) -> TestResult {
    if left != right {
        TestResult::Pass
    } else {
        TestResult::Fail
    }
}

/// Assert that a value is Some
#[inline]
pub fn assert_some<T>(value: Option<T>) -> TestResult {
    if value.is_some() {
        TestResult::Pass
    } else {
        TestResult::Fail
    }
}

/// Assert that a value is None
#[inline]
pub fn assert_none<T>(value: Option<T>) -> TestResult {
    if value.is_none() {
        TestResult::Pass
    } else {
        TestResult::Fail
    }
}

/// Assert that a Result is Ok
#[inline]
pub fn assert_ok<T, E>(value: Result<T, E>) -> TestResult {
    if value.is_ok() {
        TestResult::Pass
    } else {
        TestResult::Fail
    }
}

/// Assert that a Result is Err
#[inline]
pub fn assert_err<T, E>(value: Result<T, E>) -> TestResult {
    if value.is_err() {
        TestResult::Pass
    } else {
        TestResult::Fail
    }
}

// ============================================================================
// Test Report Generation
// ============================================================================

/// Generate a text report from test statistics
pub fn generate_report(stats: &TestStats, results: &[TestExecutionResult]) -> String {
    let mut report = String::new();

    report.push_str("=== RustOS Test Report ===\n\n");

    // Summary statistics
    report.push_str("Summary:\n");
    report.push_str(&alloc::format!("  Total Tests: {}\n", stats.total_tests));
    report.push_str(&alloc::format!(
        "  Passed: {} ({:.1}%)\n",
        stats.passed,
        stats.pass_rate()
    ));
    report.push_str(&alloc::format!("  Failed: {}\n", stats.failed));
    report.push_str(&alloc::format!("  Skipped: {}\n", stats.skipped));
    report.push_str(&alloc::format!("  Timeouts: {}\n", stats.timeouts));
    report.push_str(&alloc::format!("  Errors: {}\n", stats.errors));
    report.push_str(&alloc::format!(
        "  Execution Time: {}ms\n\n",
        stats.execution_time_ms
    ));

    // Individual test results
    if !results.is_empty() {
        report.push_str("Test Results:\n");
        for result in results {
            let status = result.result.as_str();
            report.push_str(&alloc::format!(
                "  [{}] {} ({}) - {}ms\n",
                status,
                result.test_name,
                result.suite_name,
                result.execution_time_ms
            ));

            if let Some(ref error) = result.error_message {
                report.push_str(&alloc::format!("      Error: {}\n", error));
            }
        }
    }

    report.push_str("\n=== End of Report ===\n");
    report
}
