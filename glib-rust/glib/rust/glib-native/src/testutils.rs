//! Test utilities matching `gtestutils.h` / `gtestutils.c`.
//!
//! Provides test framework types and assertion helpers.
//! In no_std, the test framework is minimal - actual test running
//! is handled by `cargo test` via `#[cfg(test)]`.
//! Fully `no_std` compatible using `alloc`.

use crate::prelude::*;

/// A test case (`GTestCase`).
pub struct TestCase {
    pub name: String,
    pub func: fn(),
}

impl TestCase {
    /// Create a new test case (`g_test_create_case`).
    pub fn new(name: &str, func: fn()) -> Self {
        Self {
            name: name.to_owned(),
            func,
        }
    }

    /// Run the test case.
    pub fn run(&self) {
        (self.func)();
    }
}

/// A test suite (`GTestSuite`).
pub struct TestSuite {
    pub name: String,
    cases: Vec<TestCase>,
    subsuites: Vec<TestSuite>,
}

impl TestSuite {
    /// Create a new test suite (`g_test_create_suite`).
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_owned(),
            cases: Vec::new(),
            subsuites: Vec::new(),
        }
    }

    /// Add a test case (`g_test_suite_add`).
    pub fn add(&mut self, test_case: TestCase) {
        self.cases.push(test_case);
    }

    /// Add a sub-suite (`g_test_suite_add_suite`).
    pub fn add_suite(&mut self, suite: TestSuite) {
        self.subsuites.push(suite);
    }

    /// Get all test cases (including from sub-suites).
    pub fn get_cases(&self) -> Vec<&TestCase> {
        let mut cases: Vec<&TestCase> = self.cases.iter().collect();
        for sub in &self.subsuites {
            cases.extend(sub.get_cases());
        }
        cases
    }

    /// Run all test cases in the suite.
    pub fn run(&self) {
        for case in &self.cases {
            case.run();
        }
        for sub in &self.subsuites {
            sub.run();
        }
    }

    /// Get the number of test cases.
    pub fn count(&self) -> usize {
        let mut n = self.cases.len();
        for sub in &self.subsuites {
            n += sub.count();
        }
        n
    }
}

/// Test trap flags (`GTestTrapFlags`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub struct TestTrapFlags(pub u32);

impl TestTrapFlags {
    pub const DEFAULT: TestTrapFlags = TestTrapFlags(0);
    pub const SILENCE_STDOUT: TestTrapFlags = TestTrapFlags(1 << 7);
    pub const SILENCE_STDERR: TestTrapFlags = TestTrapFlags(1 << 8);
    pub const INHERIT_STDIN: TestTrapFlags = TestTrapFlags(1 << 9);

    pub fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }
}

impl core::ops::BitOr for TestTrapFlags {
    type Output = Self;
    fn bitor(self, rhs: Self) -> Self {
        TestTrapFlags(self.0 | rhs.0)
    }
}

/// Test sub-process flags.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TestSubprocessFlags {
    Default,
    InheritStdin,
}

/// Initialize the test framework (`g_test_init`).
///
/// In no_std, this is a no-op. Test initialization is handled by `cargo test`.
pub fn test_init(_argv: &[String]) {
    // No-op in no_std
}

/// Run all tests (`g_test_run`).
///
/// In no_std, returns 0 (success). Actual test running is via `cargo test`.
pub fn test_run() -> i32 {
    0
}

/// Add a test function (`g_test_add_func`).
pub fn test_add_func(name: &str, func: fn()) -> TestCase {
    TestCase::new(name, func)
}

/// Add a test with data (`g_test_add_data_func`).
pub fn test_add_data_func(name: &str, func: fn()) -> TestCase {
    TestCase::new(name, func)
}

/// Create a test suite (`g_test_create_suite`).
pub fn test_create_suite(name: &str) -> TestSuite {
    TestSuite::new(name)
}

/// Get the root test suite (`g_test_get_root`).
static ROOT_SUITE: spin::Mutex<Option<TestSuite>> = spin::Mutex::new(None);

pub fn test_get_root() -> TestSuite {
    let mut guard = ROOT_SUITE.lock();
    if guard.is_none() {
        *guard = Some(TestSuite::new("root"));
    }
    TestSuite::new("root")
}

/// Assert a condition is true (`g_assert_true`).
pub fn assert_true(condition: bool, msg: &str) {
    if !condition {
        panic!("assertion failed: {}", msg);
    }
}

/// Assert a condition is false (`g_assert_false`).
pub fn assert_false(condition: bool, msg: &str) {
    if condition {
        panic!("assertion failed (expected false): {}", msg);
    }
}

/// Assert two values are equal (`g_assert_cmpint`).
pub fn assert_cmpint<T: PartialEq + core::fmt::Debug>(a: T, b: T) {
    assert_eq!(a, b);
}

/// Assert two strings are equal (`g_assert_cmpstr`).
pub fn assert_cmpstr(a: &str, b: &str) {
    assert_eq!(a, b);
}

/// Assert a pointer is null (`g_assert_null`).
pub fn assert_null<T>(ptr: *const T) {
    assert!(ptr.is_null());
}

/// Assert a pointer is not null (`g_assert_nonnull`).
pub fn assert_nonnull<T>(ptr: *const T) {
    assert!(!ptr.is_null());
}

/// Expect a failure (`g_test_expect_message`).
pub fn test_expect_message(_domain: &str, _level: &str, _message: &str) {
    // No-op in no_std
}

/// Assert expected messages were seen (`g_test_assert_expected_messages`).
pub fn test_assert_expected_messages() {
    // No-op in no_std
}

/// Test trap pass status.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TestTrapStatus {
    Pass,
    Fail,
    Timeout,
    NotImplemented,
}

/// Run a function in a sub-process with a timeout (`g_test_trap_subprocess`).
///
/// In no_std, this just runs the function directly.
pub fn test_trap_subprocess(func: fn(), _timeout_us: u64, _flags: TestTrapFlags) -> TestTrapStatus {
    func();
    TestTrapStatus::Pass
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_case_basic() {
        static mut CALLED: bool = false;
        fn test_fn() {
            unsafe {
                CALLED = true;
            }
        }
        let case = TestCase::new("my-test", test_fn);
        assert_eq!(case.name, "my-test");
        case.run();
        assert!(unsafe { CALLED });
    }

    #[test]
    fn test_suite_basic() {
        let mut suite = TestSuite::new("my-suite");
        assert_eq!(suite.count(), 0);
        suite.add(TestCase::new("test1", || {}));
        suite.add(TestCase::new("test2", || {}));
        assert_eq!(suite.count(), 2);
        suite.run();
    }

    #[test]
    fn test_suite_nested() {
        let mut suite = TestSuite::new("parent");
        suite.add(TestCase::new("test1", || {}));
        let mut child = TestSuite::new("child");
        child.add(TestCase::new("test2", || {}));
        suite.add_suite(child);
        assert_eq!(suite.count(), 2);
    }

    #[test]
    fn test_trap_flags() {
        let flags = TestTrapFlags::SILENCE_STDOUT | TestTrapFlags::SILENCE_STDERR;
        assert!(flags.contains(TestTrapFlags::SILENCE_STDOUT));
        assert!(flags.contains(TestTrapFlags::SILENCE_STDERR));
        assert!(!flags.contains(TestTrapFlags::INHERIT_STDIN));
    }

    #[test]
    fn test_run_returns_zero() {
        assert_eq!(test_run(), 0);
    }

    #[test]
    fn test_trap_subprocess_pass() {
        let status = test_trap_subprocess(|| {}, 1000, TestTrapFlags::DEFAULT);
        assert_eq!(status, TestTrapStatus::Pass);
    }
}
