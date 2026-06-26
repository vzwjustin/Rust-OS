//! Process Manager Tests
//!
//! Comprehensive test suite for process management operations.

#![cfg(test)]

use super::*;
use crate::process::Priority;

#[test_case]
fn test_process_creation() {
    let pm = ProcessManager::new();
    pm.init().expect("Failed to initialize");

    let pid = pm
        .create_process(Some(0), "test_process", Priority::Normal)
        .expect("Failed to create process");

    assert!(pid > 0);
    assert_eq!(pm.process_count(), 2); // init + test_process

    let pcb = pm.get_process(pid).expect("Process not found");
    assert_eq!(pcb.pid, pid);
    assert_eq!(pcb.parent_pid, Some(0));
    assert_eq!(pcb.name_str(), "test_process");
}

#[test_case]
fn test_fork() {
    let pm = ProcessManager::new();
    pm.init().expect("Failed to initialize");

    let parent_pid = pm
        .create_process(Some(0), "parent", Priority::Normal)
        .expect("Failed to create parent");

    let child_pid = pm.fork(parent_pid).expect("Failed to fork");

    assert!(child_pid > parent_pid);

    let child = pm.get_process(child_pid).expect("Child not found");
    assert_eq!(child.parent_pid, Some(parent_pid));
    assert_eq!(child.state, ProcessState::Ready);

    let parent = pm.get_process(parent_pid).expect("Parent not found");
    assert_eq!(parent.child_count, 1);
}

#[test_case]
fn test_exit_and_zombie() {
    let pm = ProcessManager::new();
    pm.init().expect("Failed to initialize");

    let pid = pm
        .create_process(Some(0), "test", Priority::Normal)
        .expect("Failed to create process");

    pm.exit(pid, 42).expect("Failed to exit");

    let pcb = pm.get_process(pid).expect("Process not found");
    assert_eq!(pcb.state, ProcessState::Zombie);
    assert_eq!(pcb.exit_status, Some(42));
}

#[test_case]
fn test_wait_for_child() {
    let pm = ProcessManager::new();
    pm.init().expect("Failed to initialize");

    let parent_pid = pm
        .create_process(Some(0), "parent", Priority::Normal)
        .expect("Failed to create parent");

    let child_pid = pm.fork(parent_pid).expect("Failed to fork");

    // Exit child
    pm.exit(child_pid, 123).expect("Failed to exit child");

    // Wait for child
    let (pid, status) = pm.wait(parent_pid).expect("Failed to wait");

    assert_eq!(pid, child_pid);
    assert_eq!(status, 123);

    // Child should be cleaned up
    assert!(pm.get_process(child_pid).is_none());
}

#[test_case]
fn test_waitpid_specific_child() {
    let pm = ProcessManager::new();
    pm.init().expect("Failed to initialize");

    let parent_pid = pm
        .create_process(Some(0), "parent", Priority::Normal)
        .expect("Failed to create parent");

    let child1 = pm.fork(parent_pid).expect("Failed to fork child1");
    let child2 = pm.fork(parent_pid).expect("Failed to fork child2");

    // Exit child2
    pm.exit(child2, 99).expect("Failed to exit child2");

    // Wait specifically for child2
    let status = pm.waitpid(parent_pid, child2).expect("Failed to waitpid");

    assert_eq!(status, 99);

    // child2 should be cleaned up, child1 should still exist
    assert!(pm.get_process(child2).is_none());
    assert!(pm.get_process(child1).is_some());
}

#[test_case]
fn test_file_descriptors() {
    let pm = ProcessManager::new();
    pm.init().expect("Failed to initialize");

    let pid = pm
        .create_process(Some(0), "test", Priority::Normal)
        .expect("Failed to create process");

    // Check standard FDs exist
    assert!(pm.get_fd(pid, 0).is_some()); // stdin
    assert!(pm.get_fd(pid, 1).is_some()); // stdout
    assert!(pm.get_fd(pid, 2).is_some()); // stderr

    // Allocate new FD
    let fd = pm
        .allocate_fd(pid, FileDescriptorType::File { path: [0; 256] })
        .expect("Failed to allocate FD");
    assert_eq!(fd, 3);

    // Close FD
    pm.close_fd(pid, fd).expect("Failed to close FD");
    assert!(pm.get_fd(pid, fd).is_none());

    // Cannot close standard FDs
    assert!(pm.close_fd(pid, 0).is_err());
}

#[test_case]
fn test_process_states() {
    let pm = ProcessManager::new();
    pm.init().expect("Failed to initialize");

    let pid = pm
        .create_process(Some(0), "test", Priority::Normal)
        .expect("Failed to create process");

    // Initial state is Ready
    let pcb = pm.get_process(pid).expect("Process not found");
    assert_eq!(pcb.state, ProcessState::Ready);

    // Change to Running
    pm.set_process_state(pid, ProcessState::Running)
        .expect("Failed to set state");
    let pcb = pm.get_process(pid).expect("Process not found");
    assert_eq!(pcb.state, ProcessState::Running);

    // Block process
    pm.set_process_state(pid, ProcessState::Blocked)
        .expect("Failed to set state");
    let pcb = pm.get_process(pid).expect("Process not found");
    assert_eq!(pcb.state, ProcessState::Blocked);
}

#[test_case]
fn test_process_hierarchy() {
    let pm = ProcessManager::new();
    pm.init().expect("Failed to initialize");

    let parent = pm
        .create_process(Some(0), "parent", Priority::Normal)
        .expect("Failed to create parent");

    let child1 = pm.fork(parent).expect("Failed to fork child1");
    let child2 = pm.fork(parent).expect("Failed to fork child2");
    let grandchild = pm.fork(child1).expect("Failed to fork grandchild");

    // Check parent-child relationships
    assert_eq!(pm.get_parent_pid(child1), Some(parent));
    assert_eq!(pm.get_parent_pid(child2), Some(parent));
    assert_eq!(pm.get_parent_pid(grandchild), Some(child1));
}

#[test_case]
fn test_max_processes() {
    let pm = ProcessManager::new();
    pm.init().expect("Failed to initialize");

    // Try to create many processes
    let mut created = 0;
    for i in 0..1000 {
        match pm.create_process(Some(0), "test", Priority::Normal) {
            Ok(_) => created += 1,
            Err(_) => break,
        }
    }

    assert!(created > 0);
    assert!(created <= 4096); // MAX_PROCESSES
}

#[test_case]
fn test_process_table_stats() {
    let pm = ProcessManager::new();
    pm.init().expect("Failed to initialize");

    let table = pm.process_table.lock();
    let stats = table.stats();

    assert_eq!(stats.total, 1); // Just init process
    drop(table);

    let pid1 = pm
        .create_process(Some(0), "test1", Priority::Normal)
        .expect("Failed to create");
    let pid2 = pm
        .create_process(Some(0), "test2", Priority::Normal)
        .expect("Failed to create");

    pm.set_process_state(pid1, ProcessState::Running)
        .expect("Failed to set state");
    pm.exit(pid2, 0).expect("Failed to exit");

    let table = pm.process_table.lock();
    let stats = table.stats();

    assert_eq!(stats.total, 3); // init + 2 processes
    assert_eq!(stats.running, 1);
    assert_eq!(stats.zombie, 1);
}
