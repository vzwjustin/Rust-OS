//! Comprehensive Testing Module for RustOS
//!
//! This module provides the main testing interface and coordinates all testing subsystems.
//!
//! The testing framework is defined at the crate root level (`crate::testing_framework`)
//! and this module coordinates all testing subsystems that use it.

pub mod benchmarking;
pub mod comprehensive_test_runner;
pub mod hardware_tests;
pub mod integration_tests;
pub mod production_validation;
pub mod security_tests;
pub mod stress_tests;
pub mod system_validation;

use crate::testing_framework::{
    create_default_test_suites, get_test_framework, init_testing_framework, TestStats, TestSuite,
};
use alloc::{string::String, vec::Vec};

/// Initialize the complete testing system
pub fn init_testing_system() -> Result<(), &'static str> {
    // Initialize testing framework
    init_testing_framework();

    // Initialize performance monitoring for benchmarks
    benchmarking::init_performance_monitoring()?;

    Ok(())
}

/// Run all comprehensive tests
pub fn run_comprehensive_tests() -> TestStats {
    let framework = get_test_framework();

    // Add all test suites
    let test_suites = get_all_test_suites();
    for suite in test_suites {
        framework.add_suite(suite);
    }

    // Run all tests
    framework.run_all_tests()
}

/// Get all available test suites
pub fn get_all_test_suites() -> Vec<TestSuite> {
    let mut suites = Vec::new();

    // Add unit tests
    suites.extend(create_default_test_suites());

    // Add integration tests
    suites.extend(integration_tests::get_all_integration_test_suites());

    // Add stress tests
    suites.push(stress_tests::create_stress_test_suite());

    // Add performance benchmarks
    suites.push(benchmarking::create_performance_benchmark_suite());

    // Add security tests
    suites.push(security_tests::create_security_test_suite());

    // Add hardware tests
    suites.push(hardware_tests::create_hardware_test_suite());

    suites
}

/// Run specific test category
pub fn run_test_category(category: &str) -> TestStats {
    let framework = get_test_framework();

    let suites = match category {
        "unit" => create_default_test_suites(),
        "integration" => integration_tests::get_all_integration_test_suites(),
        "stress" => alloc::vec![stress_tests::create_stress_test_suite()],
        "performance" => alloc::vec![benchmarking::create_performance_benchmark_suite()],
        "security" => alloc::vec![security_tests::create_security_test_suite()],
        "hardware" => alloc::vec![hardware_tests::create_hardware_test_suite()],
        _ => {
            return TestStats {
                total_tests: 0,
                passed: 0,
                failed: 0,
                skipped: 0,
                timeouts: 0,
                errors: 0,
                execution_time_ms: 0,
            }
        }
    };

    for suite in suites {
        framework.add_suite(suite);
    }

    framework.run_all_tests()
}

/// Get test results summary
pub fn get_test_summary() -> String {
    let framework = get_test_framework();
    let results = framework.get_results();

    let mut summary = String::new();
    summary.push_str("=== RustOS Test Results Summary ===\n");

    for result in results {
        summary.push_str(&alloc::format!(
            "{}: {} - {}ms\n",
            result.test_name,
            result.result.as_str(),
            result.execution_time_ms
        ));
    }

    summary
}
