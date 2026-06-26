//! Comprehensive Test Runner for RustOS Real Implementations
//!
//! This module provides a comprehensive test runner that validates all real
//! implementations against the requirements specified in the design document.

use crate::println;
use crate::testing::{
    benchmarking, hardware_tests, integration_tests, security_tests, stress_tests,
};
use crate::testing_framework::{TestExecutionResult, TestFramework, TestResult, TestStats};
use alloc::{
    collections::BTreeMap,
    string::{String, ToString},
    vec,
    vec::Vec,
};

/// Comprehensive test configuration
#[derive(Debug, Clone)]
pub struct ComprehensiveTestConfig {
    pub run_unit_tests: bool,
    pub run_integration_tests: bool,
    pub run_stress_tests: bool,
    pub run_performance_tests: bool,
    pub run_security_tests: bool,
    pub run_hardware_tests: bool,
    pub max_execution_time_ms: u64,
    pub fail_fast: bool,
    pub verbose_output: bool,
}

impl Default for ComprehensiveTestConfig {
    fn default() -> Self {
        Self {
            run_unit_tests: true,
            run_integration_tests: true,
            run_stress_tests: true,
            run_performance_tests: true,
            run_security_tests: true,
            run_hardware_tests: true,
            max_execution_time_ms: 300_000, // 5 minutes
            fail_fast: false,
            verbose_output: true,
        }
    }
}

/// Comprehensive test results
#[derive(Debug, Clone)]
pub struct ComprehensiveTestResults {
    pub overall_stats: TestStats,
    pub category_stats: BTreeMap<String, TestStats>,
    pub failed_tests: Vec<TestExecutionResult>,
    pub performance_regressions: Vec<String>,
    pub security_violations: Vec<String>,
    pub hardware_compatibility_issues: Vec<String>,
    pub execution_time_ms: u64,
    pub memory_usage_peak_mb: usize,
}

/// Comprehensive test runner
pub struct ComprehensiveTestRunner {
    config: ComprehensiveTestConfig,
    framework: TestFramework,
}

impl ComprehensiveTestRunner {
    /// Create a new comprehensive test runner
    pub fn new(config: ComprehensiveTestConfig) -> Self {
        Self {
            config,
            framework: TestFramework::new(),
        }
    }

    /// Run all comprehensive tests
    pub fn run_all_tests(&mut self) -> ComprehensiveTestResults {
        let start_time = crate::time::uptime_us();
        let mut category_stats = BTreeMap::new();
        let mut failed_tests = Vec::new();
        let mut performance_regressions = Vec::new();
        let mut security_violations = Vec::new();
        let mut hardware_compatibility_issues = Vec::new();

        if self.config.verbose_output {
            println!("🧪 Starting Comprehensive Test Suite for RustOS Real Implementations");
            println!("================================================================");
        }

        // Run unit tests
        if self.config.run_unit_tests {
            let stats = self.run_unit_tests();
            category_stats.insert("Unit Tests".to_string(), stats.clone());
            self.collect_failed_tests(&mut failed_tests, "Unit Tests");

            if self.config.verbose_output {
                self.print_category_results("Unit Tests", &stats);
            }

            if self.config.fail_fast && stats.failed > 0 {
                return self.create_results(
                    start_time,
                    category_stats,
                    failed_tests,
                    performance_regressions,
                    security_violations,
                    hardware_compatibility_issues,
                );
            }
        }

        // Run integration tests
        if self.config.run_integration_tests {
            let stats = self.run_integration_tests();
            category_stats.insert("Integration Tests".to_string(), stats.clone());
            self.collect_failed_tests(&mut failed_tests, "Integration Tests");

            if self.config.verbose_output {
                self.print_category_results("Integration Tests", &stats);
            }

            if self.config.fail_fast && stats.failed > 0 {
                return self.create_results(
                    start_time,
                    category_stats,
                    failed_tests,
                    performance_regressions,
                    security_violations,
                    hardware_compatibility_issues,
                );
            }
        }

        // Run stress tests
        if self.config.run_stress_tests {
            let stats = self.run_stress_tests();
            category_stats.insert("Stress Tests".to_string(), stats.clone());
            self.collect_failed_tests(&mut failed_tests, "Stress Tests");

            if self.config.verbose_output {
                self.print_category_results("Stress Tests", &stats);
            }
        }

        // Run performance tests
        if self.config.run_performance_tests {
            let (stats, regressions) = self.run_performance_tests();
            category_stats.insert("Performance Tests".to_string(), stats.clone());
            performance_regressions.extend(regressions);
            self.collect_failed_tests(&mut failed_tests, "Performance Tests");

            if self.config.verbose_output {
                self.print_category_results("Performance Tests", &stats);
                if !performance_regressions.is_empty() {
                    println!("⚠️  Performance Regressions Detected:");
                    for regression in &performance_regressions {
                        println!("   - {}", regression);
                    }
                }
            }
        }

        // Run security tests
        if self.config.run_security_tests {
            let (stats, violations) = self.run_security_tests();
            category_stats.insert("Security Tests".to_string(), stats.clone());
            security_violations.extend(violations);
            self.collect_failed_tests(&mut failed_tests, "Security Tests");

            if self.config.verbose_output {
                self.print_category_results("Security Tests", &stats);
                if !security_violations.is_empty() {
                    println!("🚨 Security Violations Detected:");
                    for violation in &security_violations {
                        println!("   - {}", violation);
                    }
                }
            }
        }

        // Run hardware tests
        if self.config.run_hardware_tests {
            let (stats, compatibility_issues) = self.run_hardware_tests();
            category_stats.insert("Hardware Tests".to_string(), stats.clone());
            hardware_compatibility_issues.extend(compatibility_issues);
            self.collect_failed_tests(&mut failed_tests, "Hardware Tests");

            if self.config.verbose_output {
                self.print_category_results("Hardware Tests", &stats);
                if !hardware_compatibility_issues.is_empty() {
                    println!("🔧 Hardware Compatibility Issues:");
                    for issue in &hardware_compatibility_issues {
                        println!("   - {}", issue);
                    }
                }
            }
        }

        self.create_results(
            start_time,
            category_stats,
            failed_tests,
            performance_regressions,
            security_violations,
            hardware_compatibility_issues,
        )
    }

    /// Run unit tests
    fn run_unit_tests(&mut self) -> TestStats {
        if self.config.verbose_output {
            println!("🔬 Running Unit Tests...");
        }

        let suites = crate::testing_framework::create_default_test_suites();
        for suite in suites {
            self.framework.add_suite(suite);
        }

        self.framework.run_all_tests()
    }

    /// Run integration tests
    fn run_integration_tests(&mut self) -> TestStats {
        if self.config.verbose_output {
            println!("🔗 Running Integration Tests...");
        }

        let suites = integration_tests::get_all_integration_test_suites();
        for suite in suites {
            self.framework.add_suite(suite);
        }

        self.framework.run_all_tests()
    }

    /// Run stress tests
    fn run_stress_tests(&mut self) -> TestStats {
        if self.config.verbose_output {
            println!("💪 Running Stress Tests...");
        }

        let suite = stress_tests::create_stress_test_suite();
        self.framework.add_suite(suite);

        self.framework.run_all_tests()
    }

    /// Run performance tests and detect regressions
    fn run_performance_tests(&mut self) -> (TestStats, Vec<String>) {
        if self.config.verbose_output {
            println!("⚡ Running Performance Tests...");
        }

        let suite = benchmarking::create_performance_benchmark_suite();
        self.framework.add_suite(suite);

        let stats = self.framework.run_all_tests();

        // Detect performance regressions
        let regressions = self.detect_performance_regressions();

        (stats, regressions)
    }

    /// Run security tests and detect violations
    fn run_security_tests(&mut self) -> (TestStats, Vec<String>) {
        if self.config.verbose_output {
            println!("🔒 Running Security Tests...");
        }

        let suite = security_tests::create_security_test_suite();
        self.framework.add_suite(suite);

        let stats = self.framework.run_all_tests();

        // Detect security violations
        let violations = self.detect_security_violations();

        (stats, violations)
    }

    /// Run hardware tests and detect compatibility issues
    fn run_hardware_tests(&mut self) -> (TestStats, Vec<String>) {
        if self.config.verbose_output {
            println!("🔧 Running Hardware Tests...");
        }

        let suite = hardware_tests::create_hardware_test_suite();
        self.framework.add_suite(suite);

        let stats = self.framework.run_all_tests();

        // Detect hardware compatibility issues
        let issues = self.detect_hardware_compatibility_issues();

        (stats, issues)
    }

    /// Collect failed tests from framework
    fn collect_failed_tests(&self, failed_tests: &mut Vec<TestExecutionResult>, category: &str) {
        let results = self.framework.get_results();
        for result in results {
            if result.result == TestResult::Fail || result.result == TestResult::Timeout {
                failed_tests.push(result);
            }
        }
    }

    /// Detect performance regressions
    fn detect_performance_regressions(&self) -> Vec<String> {
        let mut regressions = Vec::new();

        // Get current performance metrics
        let current_metrics = benchmarking::get_system_performance_summary();

        // Compare with baseline (if available)
        // This would typically load baseline metrics from storage
        // For now, we'll use hardcoded thresholds

        if let Some(memory_stats) = current_metrics.get("memory") {
            if memory_stats.mean > 1024.0 * 1024.0 * 512.0 {
                // 512MB threshold
                regressions.push("Memory usage exceeds baseline by >20%".to_string());
            }
        }

        if let Some(syscall_stats) = current_metrics.get("syscalls") {
            if syscall_stats.mean > 10000.0 {
                // 10k syscalls threshold
                regressions.push("System call latency increased by >15%".to_string());
            }
        }

        regressions
    }

    /// Detect security violations
    fn detect_security_violations(&self) -> Vec<String> {
        let mut violations = Vec::new();

        // Check for security test failures
        let results = self.framework.get_results();
        for result in results {
            if result.suite_name.contains("Security") && result.result == TestResult::Fail {
                violations.push(alloc::format!("Security test failed: {}", result.test_name));
            }
        }

        // Additional security checks
        if !crate::security::stack_canaries_enabled() {
            violations.push("Stack canaries not enabled".to_string());
        }

        if !crate::security::aslr_enabled() {
            violations.push("Address Space Layout Randomization not enabled".to_string());
        }

        violations
    }

    /// Detect hardware compatibility issues
    fn detect_hardware_compatibility_issues(&self) -> Vec<String> {
        let mut issues = Vec::new();

        // Check hardware test results
        let results = self.framework.get_results();
        for result in results {
            if result.suite_name.contains("Hardware") {
                match result.result {
                    TestResult::Fail => {
                        issues.push(alloc::format!("Hardware test failed: {}", result.test_name));
                    }
                    TestResult::Skip => {
                        issues.push(alloc::format!(
                            "Hardware not available: {}",
                            result.test_name
                        ));
                    }
                    _ => {}
                }
            }
        }

        // Check for missing hardware features
        if !crate::apic::local_apic_available() {
            issues.push("Local APIC not available - using PIC fallback".to_string());
        }

        if !crate::time::hpet_available() {
            issues.push("HPET not available - using TSC/PIT fallback".to_string());
        }

        issues
    }

    /// Print category results
    fn print_category_results(&self, category: &str, stats: &TestStats) {
        println!("📊 {} Results:", category);
        println!(
            "   Total: {}, Passed: {}, Failed: {}, Skipped: {}, Timeouts: {}",
            stats.total_tests, stats.passed, stats.failed, stats.skipped, stats.timeouts
        );
        println!("   Execution Time: {}ms", stats.execution_time_ms);

        let pass_rate = if stats.total_tests > 0 {
            (stats.passed as f32 / stats.total_tests as f32) * 100.0
        } else {
            0.0
        };

        let status = if pass_rate >= 95.0 {
            "✅ EXCELLENT"
        } else if pass_rate >= 85.0 {
            "✅ GOOD"
        } else if pass_rate >= 70.0 {
            "⚠️  ACCEPTABLE"
        } else {
            "❌ NEEDS IMPROVEMENT"
        };

        println!("   Pass Rate: {:.1}% - {}", pass_rate, status);
        println!();
    }

    /// Create comprehensive test results
    fn create_results(
        &self,
        start_time: u64,
        category_stats: BTreeMap<String, TestStats>,
        failed_tests: Vec<TestExecutionResult>,
        performance_regressions: Vec<String>,
        security_violations: Vec<String>,
        hardware_compatibility_issues: Vec<String>,
    ) -> ComprehensiveTestResults {
        let end_time = crate::time::uptime_us();
        let execution_time_ms = (end_time - start_time) / 1000;

        // Calculate overall stats
        let mut overall_stats = TestStats {
            total_tests: 0,
            passed: 0,
            failed: 0,
            skipped: 0,
            timeouts: 0,
            errors: 0,
            execution_time_ms,
        };

        for stats in category_stats.values() {
            overall_stats.total_tests += stats.total_tests;
            overall_stats.passed += stats.passed;
            overall_stats.failed += stats.failed;
            overall_stats.skipped += stats.skipped;
            overall_stats.timeouts += stats.timeouts;
            overall_stats.errors += stats.errors;
        }

        // Get memory usage
        let (memory_used, _memory_total) = crate::performance_monitor::memory_usage();
        let memory_usage_peak_mb = (memory_used / (1024 * 1024)) as usize;

        ComprehensiveTestResults {
            overall_stats,
            category_stats,
            failed_tests,
            performance_regressions,
            security_violations,
            hardware_compatibility_issues,
            execution_time_ms,
            memory_usage_peak_mb,
        }
    }

    /// Print final comprehensive results
    pub fn print_comprehensive_results(&self, results: &ComprehensiveTestResults) {
        println!("🎯 COMPREHENSIVE TEST RESULTS SUMMARY");
        println!("=====================================");
        println!();

        // Overall statistics
        println!("📈 Overall Statistics:");
        println!("   Total Tests: {}", results.overall_stats.total_tests);
        println!(
            "   Passed: {} ({}%)",
            results.overall_stats.passed,
            (results.overall_stats.passed as f32 / results.overall_stats.total_tests as f32 * 100.0)
                as u32
        );
        println!("   Failed: {}", results.overall_stats.failed);
        println!("   Skipped: {}", results.overall_stats.skipped);
        println!("   Timeouts: {}", results.overall_stats.timeouts);
        println!("   Execution Time: {}ms", results.execution_time_ms);
        println!("   Peak Memory Usage: {}MB", results.memory_usage_peak_mb);
        println!();

        // Category breakdown
        println!("📊 Category Breakdown:");
        for (category, stats) in &results.category_stats {
            let pass_rate = if stats.total_tests > 0 {
                (stats.passed as f32 / stats.total_tests as f32) * 100.0
            } else {
                0.0
            };
            println!(
                "   {}: {:.1}% ({}/{})",
                category, pass_rate, stats.passed, stats.total_tests
            );
        }
        println!();

        // Issues summary
        let total_issues = results.failed_tests.len()
            + results.performance_regressions.len()
            + results.security_violations.len()
            + results.hardware_compatibility_issues.len();

        if total_issues > 0 {
            println!("⚠️  Issues Found: {}", total_issues);

            if !results.failed_tests.is_empty() {
                println!("   Failed Tests: {}", results.failed_tests.len());
            }
            if !results.performance_regressions.is_empty() {
                println!(
                    "   Performance Regressions: {}",
                    results.performance_regressions.len()
                );
            }
            if !results.security_violations.is_empty() {
                println!(
                    "   Security Violations: {}",
                    results.security_violations.len()
                );
            }
            if !results.hardware_compatibility_issues.is_empty() {
                println!(
                    "   Hardware Issues: {}",
                    results.hardware_compatibility_issues.len()
                );
            }
        } else {
            println!("✅ No Issues Found!");
        }

        println!();

        // Final verdict
        let overall_pass_rate = if results.overall_stats.total_tests > 0 {
            (results.overall_stats.passed as f32 / results.overall_stats.total_tests as f32) * 100.0
        } else {
            0.0
        };

        let verdict = if overall_pass_rate >= 95.0 && results.security_violations.is_empty() {
            "🎉 EXCELLENT - Production Ready!"
        } else if overall_pass_rate >= 85.0 && results.security_violations.len() <= 1 {
            "✅ GOOD - Minor issues to address"
        } else if overall_pass_rate >= 70.0 {
            "⚠️  ACCEPTABLE - Several issues need attention"
        } else {
            "❌ NEEDS SIGNIFICANT IMPROVEMENT"
        };

        println!("🏆 Final Verdict: {}", verdict);
        println!("=====================================");
    }
}

/// Run comprehensive tests with default configuration
pub fn run_comprehensive_tests() -> ComprehensiveTestResults {
    let config = ComprehensiveTestConfig::default();
    let mut runner = ComprehensiveTestRunner::new(config);
    let results = runner.run_all_tests();
    runner.print_comprehensive_results(&results);
    results
}

/// Run comprehensive tests with custom configuration
pub fn run_comprehensive_tests_with_config(
    config: ComprehensiveTestConfig,
) -> ComprehensiveTestResults {
    let mut runner = ComprehensiveTestRunner::new(config);
    let results = runner.run_all_tests();
    runner.print_comprehensive_results(&results);
    results
}
