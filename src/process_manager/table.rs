//! Process Table Implementation
//!
//! Manages the collection of all processes in the system.

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, Ordering};

use super::pcb::{FileDescriptor, FileDescriptorType, ProcessControlBlock, ProcessState};
use crate::process::{Pid, Priority};

/// Maximum number of processes
const MAX_PROCESSES: usize = 4096;

/// Process Table - manages all processes in the system
pub struct ProcessTable {
    /// Map of PID to PCB
    processes: BTreeMap<Pid, ProcessControlBlock>,
    /// Next PID to allocate
    next_pid: AtomicU32,
    /// Process count
    count: usize,
}

impl ProcessTable {
    /// Create a new process table
    pub const fn new() -> Self {
        Self {
            processes: BTreeMap::new(),
            next_pid: AtomicU32::new(1),
            count: 0,
        }
    }

    /// Allocate a new PID
    pub fn allocate_pid(&mut self) -> Result<Pid, &'static str> {
        if self.count >= MAX_PROCESSES {
            return Err("Process table full");
        }

        let pid = self.next_pid.fetch_add(1, Ordering::SeqCst);

        // Check for PID wrap-around
        if pid == 0 {
            return Err("PID overflow");
        }

        Ok(pid)
    }

    /// Insert a process into the table
    pub fn insert(&mut self, pcb: ProcessControlBlock) -> Result<(), &'static str> {
        if self.count >= MAX_PROCESSES {
            return Err("Process table full");
        }

        let pid = pcb.pid;

        // Update parent's child count
        if let Some(parent_pid) = pcb.parent_pid {
            if let Some(parent) = self.processes.get_mut(&parent_pid) {
                parent.child_count += 1;
            }
        }

        self.processes.insert(pid, pcb);
        self.count += 1;
        Ok(())
    }

    /// Remove a process from the table
    pub fn remove(&mut self, pid: Pid) -> Result<(), &'static str> {
        if let Some(pcb) = self.processes.remove(&pid) {
            // Update parent's child count
            if let Some(parent_pid) = pcb.parent_pid {
                if let Some(parent) = self.processes.get_mut(&parent_pid) {
                    parent.child_count = parent.child_count.saturating_sub(1);
                }
            }

            self.count -= 1;
            Ok(())
        } else {
            Err("Process not found")
        }
    }

    /// Get a process by PID
    pub fn get(&self, pid: Pid) -> Option<ProcessControlBlock> {
        self.processes.get(&pid).cloned()
    }

    /// Get a mutable reference to a process
    pub fn get_mut(&mut self, pid: Pid) -> Option<&mut ProcessControlBlock> {
        self.processes.get_mut(&pid)
    }

    /// Check if process exists
    pub fn contains(&self, pid: Pid) -> bool {
        self.processes.contains_key(&pid)
    }

    /// Get process count
    pub fn count(&self) -> usize {
        self.count
    }

    /// List all processes
    pub fn list_all(&self) -> Vec<(Pid, String, ProcessState, Priority)> {
        self.processes
            .iter()
            .map(|(&pid, pcb)| (pid, String::from(pcb.name_str()), pcb.state, pcb.priority))
            .collect()
    }

    /// Get all children of a process
    pub fn get_children(&self, parent_pid: Pid) -> Vec<Pid> {
        self.processes
            .iter()
            .filter(|(_, pcb)| pcb.parent_pid == Some(parent_pid))
            .map(|(&pid, _)| pid)
            .collect()
    }

    /// Get zombie children of a process
    pub fn get_zombie_children(&self, parent_pid: Pid) -> Vec<Pid> {
        self.processes
            .iter()
            .filter(|(_, pcb)| pcb.parent_pid == Some(parent_pid) && pcb.is_zombie())
            .map(|(&pid, _)| pid)
            .collect()
    }

    /// Set process state
    pub fn set_state(&mut self, pid: Pid, state: ProcessState) -> Result<(), &'static str> {
        if let Some(pcb) = self.processes.get_mut(&pid) {
            pcb.set_state(state);
            Ok(())
        } else {
            Err("Process not found")
        }
    }

    /// Allocate file descriptor for process
    pub fn allocate_fd(
        &mut self,
        pid: Pid,
        fd_type: FileDescriptorType,
    ) -> Result<u32, &'static str> {
        if let Some(pcb) = self.processes.get_mut(&pid) {
            Ok(pcb.allocate_fd(fd_type))
        } else {
            Err("Process not found")
        }
    }

    /// Close file descriptor
    pub fn close_fd(&mut self, pid: Pid, fd: u32) -> Result<(), &'static str> {
        if let Some(pcb) = self.processes.get_mut(&pid) {
            pcb.close_fd(fd)
        } else {
            Err("Process not found")
        }
    }

    /// Get file descriptor
    pub fn get_fd(&self, pid: Pid, fd: u32) -> Option<FileDescriptor> {
        self.processes
            .get(&pid)
            .and_then(|pcb| pcb.get_fd(fd).cloned())
    }

    /// Get process by state
    pub fn get_by_state(&self, state: ProcessState) -> Vec<Pid> {
        self.processes
            .iter()
            .filter(|(_, pcb)| pcb.state == state)
            .map(|(&pid, _)| pid)
            .collect()
    }

    /// Count processes in a specific state
    pub fn count_by_state(&self, state: ProcessState) -> usize {
        self.processes
            .values()
            .filter(|pcb| pcb.state == state)
            .count()
    }

    /// Get orphaned processes (parent no longer exists)
    pub fn get_orphaned(&self) -> Vec<Pid> {
        self.processes
            .iter()
            .filter(|(_, pcb)| {
                if let Some(parent_pid) = pcb.parent_pid {
                    !self.processes.contains_key(&parent_pid)
                } else {
                    false
                }
            })
            .map(|(&pid, _)| pid)
            .collect()
    }

    /// Reparent orphaned processes to init (PID 1 or 0)
    pub fn reparent_orphaned(&mut self) {
        let orphaned = self.get_orphaned();
        let init_pid = if self.processes.contains_key(&1) {
            1
        } else {
            0
        };

        for pid in orphaned {
            if let Some(pcb) = self.processes.get_mut(&pid) {
                pcb.parent_pid = Some(init_pid);
            }
        }
    }

    /// Get statistics
    pub fn stats(&self) -> TableStats {
        TableStats {
            total: self.count,
            ready: self.count_by_state(ProcessState::Ready),
            running: self.count_by_state(ProcessState::Running),
            blocked: self.count_by_state(ProcessState::Blocked),
            zombie: self.count_by_state(ProcessState::Zombie),
            dead: self.count_by_state(ProcessState::Dead),
        }
    }
}

/// Process table statistics
#[derive(Debug, Clone, Copy)]
pub struct TableStats {
    pub total: usize,
    pub ready: usize,
    pub running: usize,
    pub blocked: usize,
    pub zombie: usize,
    pub dead: usize,
}
