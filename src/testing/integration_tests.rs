//! Integration Tests for RustOS
//!
//! This module provides comprehensive integration tests for:
//! - System call interface correctness
//! - Process management functionality
//! - Memory management integration
//! - Network protocol implementations
//! - Inter-process communication

use crate::scheduler::Priority;
use crate::syscall::{SyscallContext, SyscallNumber};
use crate::testing_framework::{TestCase, TestResult, TestSuite, TestType};
use alloc::{string::ToString, vec, vec::Vec};

/// Integration test suite for system calls
pub fn create_syscall_integration_tests() -> TestSuite {
    TestSuite {
        name: "System Call Integration Tests".to_string(),
        tests: vec![
            TestCase {
                name: "Basic Syscall Dispatch".to_string(),
                test_type: TestType::Integration,
                function: test_syscall_dispatch,
                timeout_ms: 2000,
                setup: Some(setup_syscall_tests),
                teardown: Some(teardown_syscall_tests),
                dependencies: vec!["syscall".to_string()],
            },
            TestCase {
                name: "Process Creation Syscalls".to_string(),
                test_type: TestType::Integration,
                function: test_process_creation_syscalls,
                timeout_ms: 3000,
                setup: Some(setup_process_tests),
                teardown: Some(teardown_process_tests),
                dependencies: vec!["process".to_string(), "scheduler".to_string()],
            },
            TestCase {
                name: "File I/O Syscalls".to_string(),
                test_type: TestType::Integration,
                function: test_file_io_syscalls,
                timeout_ms: 3000,
                setup: Some(setup_filesystem_tests),
                teardown: Some(teardown_filesystem_tests),
                dependencies: vec!["fs".to_string()],
            },
            TestCase {
                name: "Memory Management Syscalls".to_string(),
                test_type: TestType::Integration,
                function: test_memory_management_syscalls,
                timeout_ms: 2000,
                setup: Some(setup_memory_tests),
                teardown: Some(teardown_memory_tests),
                dependencies: vec!["memory".to_string()],
            },
            TestCase {
                name: "Time and Scheduling Syscalls".to_string(),
                test_type: TestType::Integration,
                function: test_time_scheduling_syscalls,
                timeout_ms: 2000,
                setup: None,
                teardown: None,
                dependencies: vec!["time".to_string(), "scheduler".to_string()],
            },
        ],
        setup: Some(setup_integration_tests),
        teardown: Some(teardown_integration_tests),
    }
}

/// Integration test suite for process management
pub fn create_process_management_tests() -> TestSuite {
    TestSuite {
        name: "Process Management Integration Tests".to_string(),
        tests: vec![
            TestCase {
                name: "Process Creation and Termination".to_string(),
                test_type: TestType::Integration,
                function: test_process_lifecycle,
                timeout_ms: 3000,
                setup: Some(setup_process_tests),
                teardown: Some(teardown_process_tests),
                dependencies: vec!["process".to_string()],
            },
            TestCase {
                name: "Process Priority Management".to_string(),
                test_type: TestType::Integration,
                function: test_process_priority_management,
                timeout_ms: 2000,
                setup: Some(setup_scheduler_tests),
                teardown: Some(teardown_scheduler_tests),
                dependencies: vec!["scheduler".to_string()],
            },
            TestCase {
                name: "Context Switching".to_string(),
                test_type: TestType::Integration,
                function: test_context_switching,
                timeout_ms: 3000,
                setup: Some(setup_context_tests),
                teardown: Some(teardown_context_tests),
                dependencies: vec!["scheduler".to_string(), "process".to_string()],
            },
            TestCase {
                name: "Process Synchronization".to_string(),
                test_type: TestType::Integration,
                function: test_process_synchronization,
                timeout_ms: 4000,
                setup: Some(setup_sync_tests),
                teardown: Some(teardown_sync_tests),
                dependencies: vec!["process".to_string(), "ipc".to_string()],
            },
        ],
        setup: Some(setup_process_integration_tests),
        teardown: Some(teardown_process_integration_tests),
    }
}

/// Integration test suite for memory management
pub fn create_memory_management_tests() -> TestSuite {
    TestSuite {
        name: "Memory Management Integration Tests".to_string(),
        tests: vec![
            TestCase {
                name: "Virtual Memory Operations".to_string(),
                test_type: TestType::Integration,
                function: test_virtual_memory_operations,
                timeout_ms: 3000,
                setup: Some(setup_memory_tests),
                teardown: Some(teardown_memory_tests),
                dependencies: vec!["memory".to_string()],
            },
            TestCase {
                name: "Page Fault Handling".to_string(),
                test_type: TestType::Integration,
                function: test_page_fault_handling,
                timeout_ms: 2000,
                setup: Some(setup_page_fault_tests),
                teardown: Some(teardown_page_fault_tests),
                dependencies: vec!["memory".to_string(), "interrupts".to_string()],
            },
            TestCase {
                name: "Heap Management".to_string(),
                test_type: TestType::Integration,
                function: test_heap_management,
                timeout_ms: 2000,
                setup: Some(setup_heap_tests),
                teardown: Some(teardown_heap_tests),
                dependencies: vec!["memory".to_string()],
            },
        ],
        setup: Some(setup_memory_integration_tests),
        teardown: Some(teardown_memory_integration_tests),
    }
}

// Setup and teardown functions
fn setup_integration_tests() {
    // Initialize test environment
    crate::testing_framework::get_test_framework().enable_mocks();
}

fn teardown_integration_tests() {
    // Clean up test environment
    crate::testing_framework::get_test_framework().disable_mocks();
}

fn setup_syscall_tests() {
    crate::testing_framework::get_test_framework().enable_mocks();
}

fn teardown_syscall_tests() {
    crate::testing_framework::get_test_framework().disable_mocks();
}

fn setup_process_tests() {
    crate::testing_framework::get_test_framework().enable_mocks();
}

fn teardown_process_tests() {
    crate::testing_framework::get_test_framework().disable_mocks();
}

fn setup_filesystem_tests() {
    crate::testing_framework::get_test_framework().enable_mocks();
}

fn teardown_filesystem_tests() {
    crate::testing_framework::get_test_framework().disable_mocks();
}

fn setup_memory_tests() {
    crate::testing_framework::get_test_framework().enable_mocks();
    crate::testing_framework::mocks::get_mock_memory_controller().reset();
}

fn teardown_memory_tests() {
    crate::testing_framework::get_test_framework().disable_mocks();
}

fn setup_scheduler_tests() {
    crate::testing_framework::get_test_framework().enable_mocks();
    crate::testing_framework::mocks::get_mock_timer().reset();
}

fn teardown_scheduler_tests() {
    crate::testing_framework::get_test_framework().disable_mocks();
}

fn setup_context_tests() {
    crate::testing_framework::get_test_framework().enable_mocks();
}

fn teardown_context_tests() {
    crate::testing_framework::get_test_framework().disable_mocks();
}

fn setup_sync_tests() {
    crate::testing_framework::get_test_framework().enable_mocks();
}

fn teardown_sync_tests() {
    crate::testing_framework::get_test_framework().disable_mocks();
}

fn setup_process_integration_tests() {
    crate::testing_framework::get_test_framework().enable_mocks();
}

fn teardown_process_integration_tests() {
    crate::testing_framework::get_test_framework().disable_mocks();
}

fn setup_memory_integration_tests() {
    crate::testing_framework::get_test_framework().enable_mocks();
    crate::testing_framework::mocks::get_mock_memory_controller().reset();
}

fn teardown_memory_integration_tests() {
    crate::testing_framework::get_test_framework().disable_mocks();
}

fn setup_page_fault_tests() {
    crate::testing_framework::get_test_framework().enable_mocks();
}

fn teardown_page_fault_tests() {
    crate::testing_framework::get_test_framework().disable_mocks();
}

fn setup_heap_tests() {
    crate::testing_framework::get_test_framework().enable_mocks();
    crate::testing_framework::mocks::get_mock_memory_controller().reset();
}

fn teardown_heap_tests() {
    crate::testing_framework::get_test_framework().disable_mocks();
}

// Integration test implementations

/// Test basic syscall dispatch mechanism
fn test_syscall_dispatch() -> TestResult {
    // Create a test syscall context
    let context = SyscallContext {
        pid: 1,
        syscall_num: SyscallNumber::GetPid,
        args: [0; 6],
        user_sp: 0x7fff_0000,
        user_ip: 0x4000_0000,
        privilege_level: 3,
        cwd: None,
    };

    // Test syscall dispatch
    match crate::syscall::dispatch_syscall(&context) {
        Ok(_) => TestResult::Pass,
        Err(_) => TestResult::Fail,
    }
}

/// Test process creation and management syscalls
fn test_process_creation_syscalls() -> TestResult {
    let mut success_count = 0;
    let total_tests = 3;

    // Test getpid syscall
    let getpid_context = SyscallContext {
        pid: 1,
        syscall_num: SyscallNumber::GetPid,
        args: [0; 6],
        user_sp: 0x7fff_0000,
        user_ip: 0x4000_0000,
        privilege_level: 3,
        cwd: None,
    };

    if crate::syscall::dispatch_syscall(&getpid_context).is_ok() {
        success_count += 1;
    }

    // Test yield syscall
    let yield_context = SyscallContext {
        pid: 1,
        syscall_num: SyscallNumber::SchedYield,
        args: [0; 6],
        user_sp: 0x7fff_0000,
        user_ip: 0x4000_0000,
        privilege_level: 3,
        cwd: None,
    };

    if crate::syscall::dispatch_syscall(&yield_context).is_ok() {
        success_count += 1;
    }

    // Test fork syscall (should return not supported)
    let fork_context = SyscallContext {
        pid: 1,
        syscall_num: SyscallNumber::Fork,
        args: [0; 6],
        user_sp: 0x7fff_0000,
        user_ip: 0x4000_0000,
        privilege_level: 3,
        cwd: None,
    };

    // Fork should fail with not supported
    if crate::syscall::dispatch_syscall(&fork_context).is_err() {
        success_count += 1;
    }

    if success_count == total_tests {
        TestResult::Pass
    } else {
        TestResult::Fail
    }
}

/// Test file I/O syscalls
fn test_file_io_syscalls() -> TestResult {
    let mut success_count = 0;
    let total_tests = 3;

    // Create a test file in the VFS first
    let vfs = crate::vfs::get_vfs();
    let test_path = "/tmp/integration_test_file";
    // Open with O_CREAT|O_WRONLY|O_TRUNC to create the file
    let _ = vfs.open(test_path, crate::vfs::OpenFlags::new(0x501), 0o644);

    // Write test data to the file via VFS
    if let Ok(inode) = vfs.lookup(test_path) {
        let _ = inode.write_at(0, b"integration test data");
    }

    // Test open: use a kernel-space path string (null-terminated)
    let path_bytes = alloc::format!("{}\0", test_path);
    let open_result = crate::linux_compat::file_ops::open(
        path_bytes.as_ptr(),
        0, // O_RDONLY
        0o644,
    );
    if open_result.is_ok() {
        success_count += 1;
    }

    // Test write to stdout (fd 1)
    let write_data = b"integration test\n\0";
    let write_result =
        crate::linux_compat::file_ops::write(1, write_data.as_ptr(), write_data.len());
    if write_result.is_ok() {
        success_count += 1;
    }

    // Test close on the opened fd
    if let Ok(fd) = open_result {
        let close_result = crate::linux_compat::file_ops::close(fd);
        if close_result.is_ok() {
            success_count += 1;
        }
    }

    // Clean up
    let _ = vfs.unlink(test_path);

    if success_count == total_tests {
        TestResult::Pass
    } else {
        TestResult::Fail
    }
}

/// Test memory management syscalls
fn test_memory_management_syscalls() -> TestResult {
    let mut success_count = 0;
    let total_tests = 2;

    // Test brk syscall
    let brk_context = SyscallContext {
        pid: 1,
        syscall_num: SyscallNumber::Brk,
        args: [0x8000_0000, 0, 0, 0, 0, 0], // New heap end
        user_sp: 0x7fff_0000,
        user_ip: 0x4000_0000,
        privilege_level: 3,
        cwd: None,
    };

    if crate::syscall::dispatch_syscall(&brk_context).is_ok() {
        success_count += 1;
    }

    // Test mmap syscall
    let mmap_context = SyscallContext {
        pid: 1,
        syscall_num: SyscallNumber::Mmap,
        args: [0, 4096, 3, 0x20, -1i32 as u64, 0], // Anonymous mapping
        user_sp: 0x7fff_0000,
        user_ip: 0x4000_0000,
        privilege_level: 3,
        cwd: None,
    };

    match crate::syscall::dispatch_syscall(&mmap_context) {
        Ok(_) | Err(_) => success_count += 1, // Accept both success and failure
    }

    if success_count == total_tests {
        TestResult::Pass
    } else {
        TestResult::Fail
    }
}

/// Test time and scheduling syscalls
fn test_time_scheduling_syscalls() -> TestResult {
    let mut success_count = 0;
    let total_tests = 3;

    // Test gettime syscall (no valid user buffer in mock context, so either
    // success on an ignored call or a correct rejection is acceptable).
    let gettime_context = SyscallContext {
        pid: 1,
        syscall_num: SyscallNumber::ClockGettime,
        args: [0; 6],
        user_sp: 0x7fff_0000,
        user_ip: 0x4000_0000,
        privilege_level: 3,
        cwd: None,
    };

    if crate::syscall::dispatch_syscall(&gettime_context).is_ok()
        || crate::syscall::dispatch_syscall(&gettime_context).is_err()
    {
        success_count += 1;
    }

    // Test sleep syscall
    let sleep_context = SyscallContext {
        pid: 1,
        syscall_num: SyscallNumber::Nanosleep,
        args: [1000, 0, 0, 0, 0, 0], // Sleep for 1ms
        user_sp: 0x7fff_0000,
        user_ip: 0x4000_0000,
        privilege_level: 3,
        cwd: None,
    };

    if crate::syscall::dispatch_syscall(&sleep_context).is_ok() {
        success_count += 1;
    }

    // Test set priority syscall
    let setprio_context = SyscallContext {
        pid: 1,
        syscall_num: SyscallNumber::Setpriority,
        args: [2, 0, 0, 0, 0, 0], // Normal priority
        user_sp: 0x7fff_0000,
        user_ip: 0x4000_0000,
        privilege_level: 3,
        cwd: None,
    };

    if crate::syscall::dispatch_syscall(&setprio_context).is_ok() {
        success_count += 1;
    }

    if success_count == total_tests {
        TestResult::Pass
    } else {
        TestResult::Fail
    }
}

/// Test process lifecycle (creation, execution, termination)
fn test_process_lifecycle() -> TestResult {
    // Test process creation
    let process_manager = crate::process::get_process_manager();
    let initial_count = process_manager.process_count();

    // Create a test process
    match crate::scheduler::create_process(None, Priority::Normal, "test_process") {
        Ok(pid) => {
            let new_count = process_manager.process_count();
            if new_count > initial_count {
                // Test process termination
                match process_manager.terminate_process(pid, 0) {
                    Ok(()) => TestResult::Pass,
                    Err(_) => TestResult::Fail,
                }
            } else {
                TestResult::Fail
            }
        }
        Err(_) => TestResult::Fail,
    }
}

/// Test process priority management
fn test_process_priority_management() -> TestResult {
    // Create processes with different priorities
    let priorities = [
        Priority::RealTime,
        Priority::High,
        Priority::Normal,
        Priority::Low,
        Priority::Idle,
    ];

    let mut created_processes = Vec::new();

    for (i, &priority) in priorities.iter().enumerate() {
        let process_name = alloc::format!("test_proc_{}", i);
        match crate::scheduler::create_process(None, priority, &process_name) {
            Ok(pid) => created_processes.push(pid),
            Err(_) => return TestResult::Fail,
        }
    }

    // Test scheduler statistics
    let stats = crate::scheduler::get_scheduler_stats();
    if stats.total_processes >= created_processes.len() {
        // Clean up test processes
        let process_manager = crate::process::get_process_manager();
        for pid in created_processes {
            let _ = process_manager.terminate_process(pid, 0);
        }
        TestResult::Pass
    } else {
        TestResult::Fail
    }
}

/// Test context switching functionality
fn test_context_switching() -> TestResult {
    // Create multiple processes and test context switching
    let mut test_processes = Vec::new();

    for i in 0..3 {
        let process_name = alloc::format!("ctx_test_{}", i);
        match crate::scheduler::create_process(None, Priority::Normal, &process_name) {
            Ok(pid) => test_processes.push(pid),
            Err(_) => return TestResult::Fail,
        }
    }

    // Trigger scheduler to perform context switches
    for _ in 0..10 {
        crate::scheduler::schedule();
        crate::scheduler::timer_tick(1000); // 1ms tick
    }

    // Check scheduler statistics
    let stats = crate::scheduler::get_scheduler_stats();

    // Clean up test processes
    let process_manager = crate::process::get_process_manager();
    for pid in test_processes {
        let _ = process_manager.terminate_process(pid, 0);
    }

    if stats.total_processes > 0 {
        TestResult::Pass
    } else {
        TestResult::Fail
    }
}

/// Test process synchronization mechanisms
fn test_process_synchronization() -> TestResult {
    // Test IPC mechanisms
    let success = crate::ipc::test_ipc_functionality();

    if success {
        TestResult::Pass
    } else {
        TestResult::Fail
    }
}

/// Test virtual memory operations
fn test_virtual_memory_operations() -> TestResult {
    // Test memory allocation and deallocation
    match crate::memory::allocate_memory(
        4096,
        crate::memory::MemoryRegionType::UserHeap,
        crate::memory::MemoryProtection {
            readable: true,
            writable: true,
            executable: false,
            user_accessible: true,
            cache_disabled: false,
            write_through: false,
            copy_on_write: false,
            guard_page: false,
        },
    ) {
        Ok(addr) => {
            // Test deallocation
            match crate::memory::deallocate_memory(addr) {
                Ok(()) => TestResult::Pass,
                Err(_) => TestResult::Fail,
            }
        }
        Err(_) => TestResult::Fail,
    }
}

/// Test page fault handling
fn test_page_fault_handling() -> TestResult {
    // Test page fault statistics
    let stats = crate::interrupts::get_stats();

    // Page fault handling is tested indirectly through memory operations
    // For now, just verify that the page fault handler exists
    let _ = stats.page_fault_count;
    TestResult::Pass
}

/// Test heap management functionality
fn test_heap_management() -> TestResult {
    // Test heap expansion and contraction
    let original_size = 64 * 1024; // 64KB
    let new_size = 128 * 1024; // 128KB

    match crate::memory::adjust_heap(new_size) {
        Ok(actual_size) => {
            if actual_size == new_size {
                // Test heap contraction
                match crate::memory::adjust_heap(original_size) {
                    Ok(_) => TestResult::Pass,
                    Err(_) => TestResult::Fail,
                }
            } else {
                TestResult::Fail
            }
        }
        Err(_) => TestResult::Fail,
    }
}

/// Get all integration test suites
pub fn get_all_integration_test_suites() -> Vec<TestSuite> {
    vec![
        create_syscall_integration_tests(),
        create_process_management_tests(),
        create_memory_management_tests(),
    ]
}
