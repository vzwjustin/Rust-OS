//! AI and GPU integration utilities.
//!
//! This module provides a lightweight orchestration layer that analyses the
//! GPUs detected by the kernel and exposes convenient helpers for scheduling
//! AI workloads.  The implementation favours clarity over hardware accuracy so
//! that higher level demos can reason about GPU/AI coordination without
//! requiring real devices.

use alloc::string::String;
use alloc::vec::Vec;
use core::cmp;
use lazy_static::lazy_static;
use spin::Mutex;

use super::{GPUCapabilities, GPUTier, GPUVendor};

lazy_static! {
    static ref AI_GPU_INTEGRATION: Mutex<AIGPUIntegration> = Mutex::new(AIGPUIntegration::new());
}

/// Detailed profile describing how suitable a GPU is for AI workloads.
#[derive(Debug, Clone)]
pub struct AIGPUProfile {
    pub name: String,
    pub vendor: GPUVendor,
    pub tier: GPUTier,
    pub compute_units: u32,
    pub memory_mb: u32,
    pub supports_ai_acceleration: bool,
    pub supports_fp16: bool,
    pub optimal_batch_size: u32,
    pub max_concurrent_streams: u16,
    pub throughput_score: u32,
}

impl AIGPUProfile {
    fn from_capabilities(cap: &GPUCapabilities) -> Self {
        let memory_mb = (cap.memory_size / (1024 * 1024)) as u32;
        let compute_units = cmp::max(cap.compute_units, 1);
        let supports_ai = cap.features.ai_acceleration
            || matches!(cap.vendor, GPUVendor::Nvidia | GPUVendor::AMD);
        let supports_fp16 = cap.features.compute_shaders;
        let throughput_score = compute_units.saturating_mul(cap.boost_clock.max(cap.base_clock));
        let optimal_batch_size = cmp::max(1, memory_mb / 256);
        let max_streams = cmp::max(1, compute_units / 16) as u16;

        Self {
            name: cap.device_name.clone(),
            vendor: cap.vendor,
            tier: cap.tier,
            compute_units,
            memory_mb,
            supports_ai_acceleration: supports_ai,
            supports_fp16,
            optimal_batch_size,
            max_concurrent_streams: max_streams,
            throughput_score,
        }
    }

    fn cpu_fallback() -> Self {
        Self {
            name: String::from("CPU Fallback"),
            vendor: GPUVendor::Unknown,
            tier: GPUTier::Entry,
            compute_units: 4,
            memory_mb: 2048,
            supports_ai_acceleration: false,
            supports_fp16: false,
            optimal_batch_size: 4,
            max_concurrent_streams: 1,
            throughput_score: 400,
        }
    }

    fn suitability_score(&self, kind: AIWorkloadKind) -> u64 {
        let mut score = self.throughput_score as u64;

        if self.supports_ai_acceleration {
            score += score / 3;
        }

        if self.supports_fp16
            && matches!(
                kind,
                AIWorkloadKind::Vision | AIWorkloadKind::Recommendation
            )
        {
            score += score / 5;
        }

        if self.memory_mb >= 8192 {
            score += 50_000;
        }

        score
    }
}

/// Different classes of workloads that can be scheduled.
#[derive(Debug, Clone, Copy)]
pub enum AIWorkloadKind {
    Vision,
    Language,
    Recommendation,
    Analytics,
}

/// Description of a scheduling request.
#[derive(Debug, Clone, Copy)]
pub struct WorkloadProfile {
    pub total_batches: u32,
    pub kind: AIWorkloadKind,
    pub realtime: bool,
}

/// Allocation of a workload to a specific GPU profile.
#[derive(Debug, Clone)]
pub struct WorkloadAssignment {
    pub gpu_name: String,
    pub batches: u32,
    pub estimated_latency_ms: u32,
}

struct AIGPUIntegration {
    profiles: Vec<AIGPUProfile>,
    total_throughput: u64,
    initialized: bool,
}

impl AIGPUIntegration {
    fn new() -> Self {
        Self {
            profiles: Vec::new(),
            total_throughput: 0,
            initialized: false,
        }
    }

    fn reset(&mut self) {
        self.profiles.clear();
        self.total_throughput = 0;
        self.initialized = false;
    }

    fn add_profile(&mut self, profile: AIGPUProfile) {
        self.total_throughput += profile.throughput_score as u64;
        self.profiles.push(profile);
    }

    fn finalise(&mut self) {
        self.profiles
            .sort_by(|a, b| b.throughput_score.cmp(&a.throughput_score));
        if self.profiles.is_empty() {
            self.add_profile(AIGPUProfile::cpu_fallback());
        }
        self.total_throughput = cmp::max(self.total_throughput, 1);
        self.initialized = true;
    }
}

/// Initialize the AI/GPU integration system with the detected GPUs.
pub fn initialize_ai_gpu_system(gpus: &[GPUCapabilities]) -> Result<(), &'static str> {
    let mut manager = AI_GPU_INTEGRATION.lock();
    manager.reset();

    for gpu in gpus {
        manager.add_profile(AIGPUProfile::from_capabilities(gpu));
    }

    manager.finalise();
    Ok(())
}

/// Returns whether the integration subsystem has been initialized.
pub fn is_initialized() -> bool {
    AI_GPU_INTEGRATION.lock().initialized
}

/// Return a snapshot of the AI ready GPU profiles.
pub fn profiles() -> Vec<AIGPUProfile> {
    AI_GPU_INTEGRATION.lock().profiles.clone()
}

/// Total compute score across all registered GPUs.
pub fn total_compute_score() -> u64 {
    AI_GPU_INTEGRATION.lock().total_throughput
}

/// Select the best suited profile for a specific workload class.
pub fn best_profile_for(kind: AIWorkloadKind) -> Option<AIGPUProfile> {
    let manager = AI_GPU_INTEGRATION.lock();
    manager
        .profiles
        .iter()
        .max_by(|a, b| a.suitability_score(kind).cmp(&b.suitability_score(kind)))
        .cloned()
}

/// Plan a workload distribution across the detected GPUs.
pub fn plan_workload(profile: WorkloadProfile) -> Vec<WorkloadAssignment> {
    let manager = AI_GPU_INTEGRATION.lock();

    if manager.profiles.is_empty() || profile.total_batches == 0 {
        return Vec::new();
    }

    let mut weights = Vec::with_capacity(manager.profiles.len());
    let mut total_weight = 0u64;
    for gpu in &manager.profiles {
        let base_weight = if profile.realtime {
            (cmp::max(gpu.max_concurrent_streams as u64, 1) * 128) + gpu.throughput_score as u64
        } else {
            gpu.throughput_score as u64
        };
        weights.push(base_weight);
        total_weight += base_weight;
    }

    if total_weight == 0 {
        return Vec::new();
    }

    let mut assignments = Vec::with_capacity(manager.profiles.len());
    let mut remaining = profile.total_batches;

    for (index, gpu) in manager.profiles.iter().enumerate() {
        let mut share = ((profile.total_batches as u64 * weights[index]) / total_weight) as u32;
        if share == 0 && remaining > 0 {
            share = 1;
        }

        if index == manager.profiles.len() - 1 {
            share = remaining;
        }

        if share > remaining {
            share = remaining;
        }

        let latency = estimate_latency_ms(gpu, share, profile.realtime);
        assignments.push(WorkloadAssignment {
            gpu_name: gpu.name.clone(),
            batches: share,
            estimated_latency_ms: latency,
        });

        if remaining <= share {
            break;
        }
        remaining -= share;
    }

    assignments
}

fn estimate_latency_ms(profile: &AIGPUProfile, batches: u32, realtime: bool) -> u32 {
    if batches == 0 {
        return 0;
    }

    let base = if realtime { 10 } else { 28 };
    let throughput = cmp::max(profile.throughput_score as u64, 1);
    let batch_factor = ((batches as u64 * 1_000) / throughput).min(90) as u32;
    let memory_bonus = cmp::min(profile.memory_mb / 1024, 12);

    base + batch_factor.saturating_sub(memory_bonus)
}
