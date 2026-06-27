//! Production x86_64 architecture support for RustOS
//!
//! Real CPU detection and architecture-specific features

use alloc::string::{String, ToString};
use core::arch::x86_64::{__cpuid, __cpuid_count, _xgetbv};

/// CPU information structure
#[derive(Debug, Clone)]
pub struct CpuInfo {
    pub vendor: String,
    pub brand: String,
    pub family: u8,
    pub model: u8,
    pub stepping: u8,
    pub max_cpuid: u32,
    pub max_extended_cpuid: u32,
}

/// CPU feature flags
#[derive(Debug, Clone, Copy)]
pub struct CpuFeatures {
    pub sse: bool,
    pub sse2: bool,
    pub sse3: bool,
    pub ssse3: bool,
    pub sse4_1: bool,
    pub sse4_2: bool,
    pub avx: bool,
    pub avx2: bool,
    pub fma: bool,
    pub bmi1: bool,
    pub bmi2: bool,
    pub popcnt: bool,
    pub rdrand: bool,
    pub rdseed: bool,
    pub fsgsbase: bool,
    pub smep: bool,
    pub smap: bool,
    pub x2apic: bool,
    pub xsave: bool,
    pub osxsave: bool,
    pub hypervisor: bool,
}

/// Global CPU information (cached)
static mut CPU_INFO: Option<CpuInfo> = None;
static mut CPU_FEATURES: Option<CpuFeatures> = None;

/// Initialize CPU detection
pub fn init() -> Result<(), &'static str> {
    detect_cpu_info();
    detect_cpu_features();
    Ok(())
}

/// Get CPU information
pub fn cpu_info() -> CpuInfo {
    unsafe {
        CPU_INFO.clone().unwrap_or_else(|| {
            detect_cpu_info();
            CPU_INFO.clone().unwrap()
        })
    }
}

/// Get CPU features
pub fn cpu_features() -> CpuFeatures {
    unsafe {
        CPU_FEATURES.unwrap_or_else(|| {
            detect_cpu_features();
            CPU_FEATURES.unwrap()
        })
    }
}

/// Detect CPU information using CPUID
fn detect_cpu_info() {
    unsafe {
        let cpuid = __cpuid(0);
        let max_cpuid = cpuid.eax;

        // Get vendor string
        let mut vendor = [0u8; 12];
        vendor[0..4].copy_from_slice(&cpuid.ebx.to_le_bytes());
        vendor[4..8].copy_from_slice(&cpuid.edx.to_le_bytes());
        vendor[8..12].copy_from_slice(&cpuid.ecx.to_le_bytes());
        let vendor_str = String::from_utf8_lossy(&vendor).to_string();

        // Get processor info
        let cpuid1 = __cpuid(1);
        let family = ((cpuid1.eax >> 8) & 0xF) as u8;
        let model = ((cpuid1.eax >> 4) & 0xF) as u8;
        let stepping = (cpuid1.eax & 0xF) as u8;

        // Get extended CPUID max
        let extended = __cpuid(0x80000000);
        let max_extended = extended.eax;

        // Get brand string if available
        let brand = if max_extended >= 0x80000004 {
            let mut brand_bytes = [0u8; 48];
            let cpuid_2 = __cpuid(0x80000002);
            let cpuid_3 = __cpuid(0x80000003);
            let cpuid_4 = __cpuid(0x80000004);

            brand_bytes[0..4].copy_from_slice(&cpuid_2.eax.to_le_bytes());
            brand_bytes[4..8].copy_from_slice(&cpuid_2.ebx.to_le_bytes());
            brand_bytes[8..12].copy_from_slice(&cpuid_2.ecx.to_le_bytes());
            brand_bytes[12..16].copy_from_slice(&cpuid_2.edx.to_le_bytes());

            brand_bytes[16..20].copy_from_slice(&cpuid_3.eax.to_le_bytes());
            brand_bytes[20..24].copy_from_slice(&cpuid_3.ebx.to_le_bytes());
            brand_bytes[24..28].copy_from_slice(&cpuid_3.ecx.to_le_bytes());
            brand_bytes[28..32].copy_from_slice(&cpuid_3.edx.to_le_bytes());

            brand_bytes[32..36].copy_from_slice(&cpuid_4.eax.to_le_bytes());
            brand_bytes[36..40].copy_from_slice(&cpuid_4.ebx.to_le_bytes());
            brand_bytes[40..44].copy_from_slice(&cpuid_4.ecx.to_le_bytes());
            brand_bytes[44..48].copy_from_slice(&cpuid_4.edx.to_le_bytes());

            String::from_utf8_lossy(&brand_bytes).trim().to_string()
        } else {
            "Unknown CPU".to_string()
        };

        CPU_INFO = Some(CpuInfo {
            vendor: vendor_str,
            brand,
            family,
            model,
            stepping,
            max_cpuid,
            max_extended_cpuid: max_extended,
        });
    }
}

/// Detect CPU features using CPUID
fn detect_cpu_features() {
    unsafe {
        let cpuid1 = __cpuid(1);
        let cpuid7 = if CPU_INFO.as_ref().map_or(false, |i| i.max_cpuid >= 7) {
            __cpuid_count(7, 0)
        } else {
            core::mem::zeroed()
        };

        let features = CpuFeatures {
            // CPUID.01H:EDX
            sse: cpuid1.edx & (1 << 25) != 0,
            sse2: cpuid1.edx & (1 << 26) != 0,

            // CPUID.01H:ECX
            sse3: cpuid1.ecx & (1 << 0) != 0,
            ssse3: cpuid1.ecx & (1 << 9) != 0,
            fma: cpuid1.ecx & (1 << 12) != 0,
            sse4_1: cpuid1.ecx & (1 << 19) != 0,
            sse4_2: cpuid1.ecx & (1 << 20) != 0,
            x2apic: cpuid1.ecx & (1 << 21) != 0,
            popcnt: cpuid1.ecx & (1 << 23) != 0,
            xsave: cpuid1.ecx & (1 << 26) != 0,
            osxsave: cpuid1.ecx & (1 << 27) != 0,
            avx: cpuid1.ecx & (1 << 28) != 0,
            rdrand: cpuid1.ecx & (1 << 30) != 0,
            hypervisor: cpuid1.ecx & (1 << 31) != 0,

            // CPUID.07H:EBX
            fsgsbase: cpuid7.ebx & (1 << 0) != 0,
            bmi1: cpuid7.ebx & (1 << 3) != 0,
            avx2: cpuid7.ebx & (1 << 5) != 0,
            smep: cpuid7.ebx & (1 << 7) != 0,
            bmi2: cpuid7.ebx & (1 << 8) != 0,
            rdseed: cpuid7.ebx & (1 << 18) != 0,
            smap: cpuid7.ebx & (1 << 20) != 0,
        };

        CPU_FEATURES = Some(features);
    }
}

/// Check if CPU supports AVX
pub fn has_avx() -> bool {
    let features = cpu_features();
    features.avx && features.osxsave && {
        // Check if OS has enabled AVX
        unsafe { (_xgetbv(0) & 0x6) == 0x6 }
    }
}

/// Get current CPU ID (APIC ID)
pub fn current_cpu_id() -> u32 {
    let cpuid = __cpuid(1);
    (cpuid.ebx >> 24) as u32
}

/// CPU relax hint (PAUSE instruction)
#[inline(always)]
pub fn cpu_relax() {
    unsafe {
        core::arch::asm!("pause");
    }
}
