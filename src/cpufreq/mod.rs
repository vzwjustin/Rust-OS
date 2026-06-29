//! CPU frequency scaling framework with sysfs exposure.
//!
//! Registers per-CPU policies, governors, and publishes scaling attributes
//! under `/sys/devices/system/cpu/cpuN/cpufreq/`.

extern crate alloc;

use alloc::collections::BTreeMap;
use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec;
use alloc::vec::Vec;
use spin::RwLock;

/// Default BSP frequency when hardware reporting is unavailable (kHz).
const DEFAULT_FREQ_KHZ: u32 = 2_400_000;

/// Known scaling governors.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Governor {
    Performance,
    Powersave,
    Ondemand,
    Conservative,
    Schedutil,
    Userspace,
}

impl Governor {
    pub fn name(self) -> &'static str {
        match self {
            Self::Performance => "performance",
            Self::Powersave => "powersave",
            Self::Ondemand => "ondemand",
            Self::Conservative => "conservative",
            Self::Schedutil => "schedutil",
            Self::Userspace => "userspace",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim() {
            "performance" => Some(Self::Performance),
            "powersave" => Some(Self::Powersave),
            "ondemand" => Some(Self::Ondemand),
            "conservative" => Some(Self::Conservative),
            "schedutil" => Some(Self::Schedutil),
            "userspace" => Some(Self::Userspace),
            _ => None,
        }
    }
}

/// Per-CPU frequency policy.
#[derive(Debug, Clone)]
pub struct CpufreqPolicy {
    pub cpu: u32,
    pub min_freq_khz: u32,
    pub max_freq_khz: u32,
    pub cur_freq_khz: u32,
    pub governor: Governor,
    pub available_freqs: Vec<u32>,
}

impl CpufreqPolicy {
    fn bootstrap(cpu: u32) -> Self {
        let freqs = vec![
            DEFAULT_FREQ_KHZ / 2,
            DEFAULT_FREQ_KHZ * 3 / 4,
            DEFAULT_FREQ_KHZ,
        ];
        Self {
            cpu,
            min_freq_khz: freqs[0],
            max_freq_khz: freqs[freqs.len() - 1],
            cur_freq_khz: freqs[freqs.len() - 1],
            governor: Governor::Ondemand,
            available_freqs: freqs,
        }
    }

    fn apply_governor(&mut self) {
        self.cur_freq_khz = match self.governor {
            Governor::Performance | Governor::Userspace => self.max_freq_khz,
            Governor::Powersave => self.min_freq_khz,
            Governor::Ondemand | Governor::Conservative | Governor::Schedutil => {
                let stats = crate::scheduler::get_scheduler_stats();
                let load = stats.ready_processes.saturating_mul(32) as u32 + 64;
                if load > 200 {
                    self.max_freq_khz
                } else if load > 64 {
                    self.available_freqs[self.available_freqs.len() / 2]
                } else {
                    self.min_freq_khz
                }
            }
        };
    }
}

static POLICIES: RwLock<BTreeMap<u32, CpufreqPolicy>> = RwLock::new(BTreeMap::new());

/// Register or replace a CPU frequency policy.
pub fn register_policy(policy: CpufreqPolicy) {
    POLICIES.write().insert(policy.cpu, policy);
}

/// Lookup policy for a CPU.
pub fn get_policy(cpu: u32) -> Option<CpufreqPolicy> {
    POLICIES.read().get(&cpu).cloned()
}

/// Set governor by name; returns false if CPU or governor is unknown.
pub fn set_governor(cpu: u32, name: &str) -> bool {
    let gov = match Governor::parse(name) {
        Some(g) => g,
        None => return false,
    };
    let mut policies = POLICIES.write();
    let Some(policy) = policies.get_mut(&cpu) else {
        return false;
    };
    policy.governor = gov;
    policy.apply_governor();
    publish_sysfs_cpu(cpu, policy);
    true
}

/// Set userspace target frequency (must be within min/max).
pub fn set_frequency(cpu: u32, freq_khz: u32) -> bool {
    let mut policies = POLICIES.write();
    let Some(policy) = policies.get_mut(&cpu) else {
        return false;
    };
    if freq_khz < policy.min_freq_khz || freq_khz > policy.max_freq_khz {
        return false;
    }
    policy.cur_freq_khz = freq_khz;
    policy.governor = Governor::Userspace;
    publish_sysfs_cpu(cpu, policy);
    true
}

fn publish_sysfs_cpu(cpu: u32, policy: &CpufreqPolicy) {
    let base = format!("devices/system/cpu/cpu{}/cpufreq", cpu);
    let freqs: String = policy
        .available_freqs
        .iter()
        .map(|f| f.to_string())
        .collect::<Vec<_>>()
        .join(" ");
    let _ = crate::fs::update_sysfs_file(
        &format!("{}/scaling_available_frequencies", base),
        format!("{}\n", freqs).as_bytes(),
    );
    let _ = crate::fs::update_sysfs_file(
        &format!("{}/scaling_governor", base),
        format!("{}\n", policy.governor.name()).as_bytes(),
    );
    let _ = crate::fs::update_sysfs_file(
        &format!("{}/scaling_cur_freq", base),
        format!("{}\n", policy.cur_freq_khz).as_bytes(),
    );
    let _ = crate::fs::update_sysfs_file(
        &format!("{}/scaling_min_freq", base),
        format!("{}\n", policy.min_freq_khz).as_bytes(),
    );
    let _ = crate::fs::update_sysfs_file(
        &format!("{}/scaling_max_freq", base),
        format!("{}\n", policy.max_freq_khz).as_bytes(),
    );
}

/// Re-evaluate ondemand-style governors for all CPUs (called from timer tick).
pub fn update_policies() {
    let cpus: Vec<u32> = POLICIES.read().keys().copied().collect();
    for cpu in cpus {
        let mut policies = POLICIES.write();
        if let Some(policy) = policies.get_mut(&cpu) {
            if matches!(
                policy.governor,
                Governor::Ondemand | Governor::Conservative | Governor::Schedutil
            ) {
                policy.apply_governor();
                publish_sysfs_cpu(cpu, policy);
            }
        }
    }
}

/// Initialize cpufreq policies for all online CPUs and publish sysfs nodes.
pub fn init() {
    let count = crate::smp::cpu_count().max(1);
    for cpu in 0..count {
        if cpu == 0 || crate::smp::is_cpu_online(cpu) {
            let mut policy = CpufreqPolicy::bootstrap(cpu);
            policy.apply_governor();
            publish_sysfs_cpu(cpu, &policy);
            POLICIES.write().insert(cpu, policy);
        }
    }
    let gov_list = "performance powersave ondemand conservative schedutil userspace\n";
    let _ = crate::fs::update_sysfs_file(
        "devices/system/cpu/cpufreq/policy0/scaling_available_governors",
        gov_list.as_bytes(),
    );
    crate::serial_println!("[cpufreq] initialized ({} policies)", POLICIES.read().len());
}
