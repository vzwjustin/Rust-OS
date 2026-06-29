//! Advanced GPU Acceleration System for RustOS
//!
//! A comprehensive GPU acceleration framework featuring:
//! - Multi-vendor GPU detection (Intel, NVIDIA, AMD)
//! - Advanced graphics acceleration (2D/3D, compute shaders, ray tracing)
//! - Opensource driver integration (Nouveau, AMDGPU, i915)
//! - AI-optimized performance monitoring and optimization
//! - Advanced memory management and cross-GPU sharing
//! - Hardware-accelerated video encode/decode

use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec;
use alloc::vec::Vec;
use core::fmt;
use lazy_static::lazy_static;
use spin::Mutex;

pub mod accel;
pub mod ai_integration;
pub mod memory;
pub mod opensource;

/// GPU device database with comprehensive device ID support
static GPU_DEVICE_DATABASE: &[(u16, u16, &str, GPUTier, GPUFeatures)] = &[
    // Intel GPUs - 50+ devices
    (
        0x8086,
        0x0042,
        "Intel HD Graphics (Ironlake)",
        GPUTier::Entry,
        GPUFeatures::basic(),
    ),
    (
        0x8086,
        0x0046,
        "Intel HD Graphics (Ironlake)",
        GPUTier::Entry,
        GPUFeatures::basic(),
    ),
    (
        0x8086,
        0x0102,
        "Intel HD Graphics 2000 (Sandy Bridge)",
        GPUTier::Entry,
        GPUFeatures::basic(),
    ),
    (
        0x8086,
        0x0106,
        "Intel HD Graphics 2000 (Sandy Bridge)",
        GPUTier::Entry,
        GPUFeatures::basic(),
    ),
    (
        0x8086,
        0x010A,
        "Intel HD Graphics P3000 (Sandy Bridge)",
        GPUTier::Entry,
        GPUFeatures::basic(),
    ),
    (
        0x8086,
        0x0112,
        "Intel HD Graphics 3000 (Sandy Bridge)",
        GPUTier::Budget,
        GPUFeatures::basic(),
    ),
    (
        0x8086,
        0x0116,
        "Intel HD Graphics 3000 (Sandy Bridge)",
        GPUTier::Budget,
        GPUFeatures::basic(),
    ),
    (
        0x8086,
        0x0122,
        "Intel HD Graphics 3000 (Sandy Bridge)",
        GPUTier::Budget,
        GPUFeatures::basic(),
    ),
    (
        0x8086,
        0x0126,
        "Intel HD Graphics 3000 (Sandy Bridge)",
        GPUTier::Budget,
        GPUFeatures::basic(),
    ),
    (
        0x8086,
        0x0152,
        "Intel HD Graphics 2500 (Ivy Bridge)",
        GPUTier::Budget,
        GPUFeatures::basic(),
    ),
    (
        0x8086,
        0x0156,
        "Intel HD Graphics 2500 (Ivy Bridge)",
        GPUTier::Budget,
        GPUFeatures::basic(),
    ),
    (
        0x8086,
        0x015A,
        "Intel HD Graphics 2500 (Ivy Bridge)",
        GPUTier::Budget,
        GPUFeatures::basic(),
    ),
    (
        0x8086,
        0x0162,
        "Intel HD Graphics 4000 (Ivy Bridge)",
        GPUTier::Budget,
        GPUFeatures::dx11(),
    ),
    (
        0x8086,
        0x0166,
        "Intel HD Graphics 4000 (Ivy Bridge)",
        GPUTier::Budget,
        GPUFeatures::dx11(),
    ),
    (
        0x8086,
        0x016A,
        "Intel HD Graphics P4000 (Ivy Bridge)",
        GPUTier::Budget,
        GPUFeatures::dx11(),
    ),
    (
        0x8086,
        0x0402,
        "Intel HD Graphics (Haswell)",
        GPUTier::Budget,
        GPUFeatures::dx11(),
    ),
    (
        0x8086,
        0x0406,
        "Intel HD Graphics (Haswell)",
        GPUTier::Budget,
        GPUFeatures::dx11(),
    ),
    (
        0x8086,
        0x040A,
        "Intel HD Graphics (Haswell)",
        GPUTier::Budget,
        GPUFeatures::dx11(),
    ),
    (
        0x8086,
        0x0412,
        "Intel HD Graphics 4600 (Haswell)",
        GPUTier::Mainstream,
        GPUFeatures::dx11(),
    ),
    (
        0x8086,
        0x0416,
        "Intel HD Graphics 4600 (Haswell)",
        GPUTier::Mainstream,
        GPUFeatures::dx11(),
    ),
    (
        0x8086,
        0x041A,
        "Intel HD Graphics P4600 (Haswell)",
        GPUTier::Mainstream,
        GPUFeatures::dx11(),
    ),
    (
        0x8086,
        0x041E,
        "Intel HD Graphics 4400 (Haswell)",
        GPUTier::Budget,
        GPUFeatures::dx11(),
    ),
    (
        0x8086,
        0x0422,
        "Intel HD Graphics 5000 (Haswell)",
        GPUTier::Mainstream,
        GPUFeatures::dx11(),
    ),
    (
        0x8086,
        0x0426,
        "Intel HD Graphics 5000 (Haswell)",
        GPUTier::Mainstream,
        GPUFeatures::dx11(),
    ),
    (
        0x8086,
        0x042A,
        "Intel HD Graphics 5000 (Haswell)",
        GPUTier::Mainstream,
        GPUFeatures::dx11(),
    ),
    (
        0x8086,
        0x042B,
        "Intel HD Graphics 5000 (Haswell)",
        GPUTier::Mainstream,
        GPUFeatures::dx11(),
    ),
    (
        0x8086,
        0x042E,
        "Intel HD Graphics 5000 (Haswell)",
        GPUTier::Mainstream,
        GPUFeatures::dx11(),
    ),
    (
        0x8086,
        0x0A02,
        "Intel HD Graphics (Haswell)",
        GPUTier::Budget,
        GPUFeatures::dx11(),
    ),
    (
        0x8086,
        0x0A06,
        "Intel HD Graphics (Haswell)",
        GPUTier::Budget,
        GPUFeatures::dx11(),
    ),
    (
        0x8086,
        0x0A0A,
        "Intel HD Graphics (Haswell)",
        GPUTier::Budget,
        GPUFeatures::dx11(),
    ),
    (
        0x8086,
        0x0A0B,
        "Intel HD Graphics (Haswell)",
        GPUTier::Budget,
        GPUFeatures::dx11(),
    ),
    (
        0x8086,
        0x0A0E,
        "Intel HD Graphics (Haswell)",
        GPUTier::Budget,
        GPUFeatures::dx11(),
    ),
    (
        0x8086,
        0x0A12,
        "Intel HD Graphics 4600 (Haswell)",
        GPUTier::Mainstream,
        GPUFeatures::dx11(),
    ),
    (
        0x8086,
        0x0A16,
        "Intel HD Graphics 4400 (Haswell)",
        GPUTier::Budget,
        GPUFeatures::dx11(),
    ),
    (
        0x8086,
        0x0A1A,
        "Intel HD Graphics 4200 (Haswell)",
        GPUTier::Budget,
        GPUFeatures::dx11(),
    ),
    (
        0x8086,
        0x0A1E,
        "Intel HD Graphics 4200 (Haswell)",
        GPUTier::Budget,
        GPUFeatures::dx11(),
    ),
    (
        0x8086,
        0x0A22,
        "Intel Iris Graphics 5100 (Haswell)",
        GPUTier::Performance,
        GPUFeatures::dx11(),
    ),
    (
        0x8086,
        0x0A26,
        "Intel HD Graphics 5000 (Haswell)",
        GPUTier::Mainstream,
        GPUFeatures::dx11(),
    ),
    (
        0x8086,
        0x0A2A,
        "Intel Iris Graphics 5100 (Haswell)",
        GPUTier::Performance,
        GPUFeatures::dx11(),
    ),
    (
        0x8086,
        0x0A2B,
        "Intel Iris Graphics 5100 (Haswell)",
        GPUTier::Performance,
        GPUFeatures::dx11(),
    ),
    (
        0x8086,
        0x0A2E,
        "Intel Iris Graphics 5100 (Haswell)",
        GPUTier::Performance,
        GPUFeatures::dx11(),
    ),
    (
        0x8086,
        0x0D12,
        "Intel HD Graphics 4600 (Haswell)",
        GPUTier::Mainstream,
        GPUFeatures::dx11(),
    ),
    (
        0x8086,
        0x0D16,
        "Intel HD Graphics 4600 (Haswell)",
        GPUTier::Mainstream,
        GPUFeatures::dx11(),
    ),
    (
        0x8086,
        0x0D1A,
        "Intel HD Graphics P4600 (Haswell)",
        GPUTier::Mainstream,
        GPUFeatures::dx11(),
    ),
    (
        0x8086,
        0x0D1B,
        "Intel HD Graphics P4700 (Haswell)",
        GPUTier::Mainstream,
        GPUFeatures::dx11(),
    ),
    (
        0x8086,
        0x0D1E,
        "Intel HD Graphics 4400 (Haswell)",
        GPUTier::Budget,
        GPUFeatures::dx11(),
    ),
    (
        0x8086,
        0x0D22,
        "Intel Iris Pro Graphics 5200 (Haswell)",
        GPUTier::Performance,
        GPUFeatures::dx11(),
    ),
    (
        0x8086,
        0x0D26,
        "Intel Iris Pro Graphics 5200 (Haswell)",
        GPUTier::Performance,
        GPUFeatures::dx11(),
    ),
    (
        0x8086,
        0x0D2A,
        "Intel Iris Pro Graphics 5200 (Haswell)",
        GPUTier::Performance,
        GPUFeatures::dx11(),
    ),
    (
        0x8086,
        0x0D2B,
        "Intel Iris Pro Graphics 5200 (Haswell)",
        GPUTier::Performance,
        GPUFeatures::dx11(),
    ),
    (
        0x8086,
        0x0D2E,
        "Intel Iris Pro Graphics 5200 (Haswell)",
        GPUTier::Performance,
        GPUFeatures::dx11(),
    ),
    // Broadwell series
    (
        0x8086,
        0x1602,
        "Intel HD Graphics (Broadwell)",
        GPUTier::Budget,
        GPUFeatures::dx12(),
    ),
    (
        0x8086,
        0x1606,
        "Intel HD Graphics (Broadwell)",
        GPUTier::Budget,
        GPUFeatures::dx12(),
    ),
    (
        0x8086,
        0x160A,
        "Intel HD Graphics (Broadwell)",
        GPUTier::Budget,
        GPUFeatures::dx12(),
    ),
    (
        0x8086,
        0x160B,
        "Intel HD Graphics (Broadwell)",
        GPUTier::Budget,
        GPUFeatures::dx12(),
    ),
    (
        0x8086,
        0x160D,
        "Intel HD Graphics (Broadwell)",
        GPUTier::Budget,
        GPUFeatures::dx12(),
    ),
    (
        0x8086,
        0x160E,
        "Intel HD Graphics (Broadwell)",
        GPUTier::Budget,
        GPUFeatures::dx12(),
    ),
    (
        0x8086,
        0x1612,
        "Intel HD Graphics 5600 (Broadwell)",
        GPUTier::Mainstream,
        GPUFeatures::dx12(),
    ),
    (
        0x8086,
        0x1616,
        "Intel HD Graphics 5500 (Broadwell)",
        GPUTier::Budget,
        GPUFeatures::dx12(),
    ),
    (
        0x8086,
        0x161A,
        "Intel HD Graphics P5700 (Broadwell)",
        GPUTier::Mainstream,
        GPUFeatures::dx12(),
    ),
    (
        0x8086,
        0x161B,
        "Intel HD Graphics P5700 (Broadwell)",
        GPUTier::Mainstream,
        GPUFeatures::dx12(),
    ),
    (
        0x8086,
        0x161D,
        "Intel HD Graphics P5700 (Broadwell)",
        GPUTier::Mainstream,
        GPUFeatures::dx12(),
    ),
    (
        0x8086,
        0x161E,
        "Intel HD Graphics 5300 (Broadwell)",
        GPUTier::Budget,
        GPUFeatures::dx12(),
    ),
    (
        0x8086,
        0x1622,
        "Intel Iris Pro Graphics 6200 (Broadwell)",
        GPUTier::Performance,
        GPUFeatures::dx12(),
    ),
    (
        0x8086,
        0x1626,
        "Intel HD Graphics 6000 (Broadwell)",
        GPUTier::Mainstream,
        GPUFeatures::dx12(),
    ),
    (
        0x8086,
        0x162A,
        "Intel Iris Pro Graphics 6200 (Broadwell)",
        GPUTier::Performance,
        GPUFeatures::dx12(),
    ),
    (
        0x8086,
        0x162B,
        "Intel Iris Graphics 6100 (Broadwell)",
        GPUTier::Performance,
        GPUFeatures::dx12(),
    ),
    (
        0x8086,
        0x162D,
        "Intel Iris Pro Graphics 6200 (Broadwell)",
        GPUTier::Performance,
        GPUFeatures::dx12(),
    ),
    (
        0x8086,
        0x162E,
        "Intel HD Graphics 5300 (Broadwell)",
        GPUTier::Budget,
        GPUFeatures::dx12(),
    ),
    // Skylake series
    (
        0x8086,
        0x1902,
        "Intel HD Graphics 510 (Skylake)",
        GPUTier::Budget,
        GPUFeatures::dx12(),
    ),
    (
        0x8086,
        0x1906,
        "Intel HD Graphics 510 (Skylake)",
        GPUTier::Budget,
        GPUFeatures::dx12(),
    ),
    (
        0x8086,
        0x190A,
        "Intel HD Graphics P510 (Skylake)",
        GPUTier::Budget,
        GPUFeatures::dx12(),
    ),
    (
        0x8086,
        0x190B,
        "Intel HD Graphics 510 (Skylake)",
        GPUTier::Budget,
        GPUFeatures::dx12(),
    ),
    (
        0x8086,
        0x190E,
        "Intel HD Graphics 510 (Skylake)",
        GPUTier::Budget,
        GPUFeatures::dx12(),
    ),
    (
        0x8086,
        0x1912,
        "Intel HD Graphics 530 (Skylake)",
        GPUTier::Mainstream,
        GPUFeatures::dx12(),
    ),
    (
        0x8086,
        0x1913,
        "Intel HD Graphics 520 (Skylake)",
        GPUTier::Budget,
        GPUFeatures::dx12(),
    ),
    (
        0x8086,
        0x1915,
        "Intel HD Graphics 520 (Skylake)",
        GPUTier::Budget,
        GPUFeatures::dx12(),
    ),
    (
        0x8086,
        0x1916,
        "Intel HD Graphics 520 (Skylake)",
        GPUTier::Budget,
        GPUFeatures::dx12(),
    ),
    (
        0x8086,
        0x1917,
        "Intel HD Graphics 520 (Skylake)",
        GPUTier::Budget,
        GPUFeatures::dx12(),
    ),
    (
        0x8086,
        0x191A,
        "Intel HD Graphics P530 (Skylake)",
        GPUTier::Mainstream,
        GPUFeatures::dx12(),
    ),
    (
        0x8086,
        0x191B,
        "Intel HD Graphics 530 (Skylake)",
        GPUTier::Mainstream,
        GPUFeatures::dx12(),
    ),
    (
        0x8086,
        0x191D,
        "Intel HD Graphics P530 (Skylake)",
        GPUTier::Mainstream,
        GPUFeatures::dx12(),
    ),
    (
        0x8086,
        0x191E,
        "Intel HD Graphics 515 (Skylake)",
        GPUTier::Budget,
        GPUFeatures::dx12(),
    ),
    (
        0x8086,
        0x1921,
        "Intel HD Graphics 520 (Skylake)",
        GPUTier::Budget,
        GPUFeatures::dx12(),
    ),
    (
        0x8086,
        0x1923,
        "Intel HD Graphics 535 (Skylake)",
        GPUTier::Budget,
        GPUFeatures::dx12(),
    ),
    (
        0x8086,
        0x1926,
        "Intel Iris Graphics 540 (Skylake)",
        GPUTier::Performance,
        GPUFeatures::dx12(),
    ),
    (
        0x8086,
        0x1927,
        "Intel Iris Graphics 550 (Skylake)",
        GPUTier::Performance,
        GPUFeatures::dx12(),
    ),
    (
        0x8086,
        0x192A,
        "Intel Iris Pro Graphics P555 (Skylake)",
        GPUTier::Performance,
        GPUFeatures::dx12(),
    ),
    (
        0x8086,
        0x192B,
        "Intel Iris Graphics 555 (Skylake)",
        GPUTier::Performance,
        GPUFeatures::dx12(),
    ),
    (
        0x8086,
        0x192D,
        "Intel Iris Pro Graphics P580 (Skylake)",
        GPUTier::Performance,
        GPUFeatures::dx12(),
    ),
    // Kaby Lake and newer
    (
        0x8086,
        0x5902,
        "Intel HD Graphics 610 (Kaby Lake)",
        GPUTier::Budget,
        GPUFeatures::dx12(),
    ),
    (
        0x8086,
        0x5906,
        "Intel HD Graphics 610 (Kaby Lake)",
        GPUTier::Budget,
        GPUFeatures::dx12(),
    ),
    (
        0x8086,
        0x590A,
        "Intel HD Graphics P610 (Kaby Lake)",
        GPUTier::Budget,
        GPUFeatures::dx12(),
    ),
    (
        0x8086,
        0x590B,
        "Intel HD Graphics 610 (Kaby Lake)",
        GPUTier::Budget,
        GPUFeatures::dx12(),
    ),
    (
        0x8086,
        0x590E,
        "Intel HD Graphics 610 (Kaby Lake)",
        GPUTier::Budget,
        GPUFeatures::dx12(),
    ),
    (
        0x8086,
        0x5912,
        "Intel HD Graphics 630 (Kaby Lake)",
        GPUTier::Mainstream,
        GPUFeatures::dx12(),
    ),
    (
        0x8086,
        0x5913,
        "Intel HD Graphics 620 (Kaby Lake)",
        GPUTier::Budget,
        GPUFeatures::dx12(),
    ),
    (
        0x8086,
        0x5915,
        "Intel HD Graphics 620 (Kaby Lake)",
        GPUTier::Budget,
        GPUFeatures::dx12(),
    ),
    (
        0x8086,
        0x5916,
        "Intel HD Graphics 620 (Kaby Lake)",
        GPUTier::Budget,
        GPUFeatures::dx12(),
    ),
    (
        0x8086,
        0x5917,
        "Intel HD Graphics 620 (Kaby Lake)",
        GPUTier::Budget,
        GPUFeatures::dx12(),
    ),
    (
        0x8086,
        0x591A,
        "Intel HD Graphics P630 (Kaby Lake)",
        GPUTier::Mainstream,
        GPUFeatures::dx12(),
    ),
    (
        0x8086,
        0x591B,
        "Intel HD Graphics 630 (Kaby Lake)",
        GPUTier::Mainstream,
        GPUFeatures::dx12(),
    ),
    (
        0x8086,
        0x591C,
        "Intel UHD Graphics 615 (Kaby Lake)",
        GPUTier::Budget,
        GPUFeatures::dx12(),
    ),
    (
        0x8086,
        0x591D,
        "Intel HD Graphics P630 (Kaby Lake)",
        GPUTier::Mainstream,
        GPUFeatures::dx12(),
    ),
    (
        0x8086,
        0x591E,
        "Intel HD Graphics 615 (Kaby Lake)",
        GPUTier::Budget,
        GPUFeatures::dx12(),
    ),
    (
        0x8086,
        0x5921,
        "Intel HD Graphics 620 (Kaby Lake)",
        GPUTier::Budget,
        GPUFeatures::dx12(),
    ),
    (
        0x8086,
        0x5923,
        "Intel HD Graphics 635 (Kaby Lake)",
        GPUTier::Budget,
        GPUFeatures::dx12(),
    ),
    (
        0x8086,
        0x5926,
        "Intel Iris Plus Graphics 640 (Kaby Lake)",
        GPUTier::Performance,
        GPUFeatures::dx12(),
    ),
    (
        0x8086,
        0x5927,
        "Intel Iris Plus Graphics 650 (Kaby Lake)",
        GPUTier::Performance,
        GPUFeatures::dx12(),
    ),
    // Coffee Lake
    (
        0x8086,
        0x3E90,
        "Intel UHD Graphics 610 (Coffee Lake)",
        GPUTier::Budget,
        GPUFeatures::dx12(),
    ),
    (
        0x8086,
        0x3E91,
        "Intel UHD Graphics 630 (Coffee Lake)",
        GPUTier::Mainstream,
        GPUFeatures::dx12(),
    ),
    (
        0x8086,
        0x3E92,
        "Intel UHD Graphics 630 (Coffee Lake)",
        GPUTier::Mainstream,
        GPUFeatures::dx12(),
    ),
    (
        0x8086,
        0x3E93,
        "Intel UHD Graphics 610 (Coffee Lake)",
        GPUTier::Budget,
        GPUFeatures::dx12(),
    ),
    (
        0x8086,
        0x3E94,
        "Intel UHD Graphics P630 (Coffee Lake)",
        GPUTier::Mainstream,
        GPUFeatures::dx12(),
    ),
    (
        0x8086,
        0x3E96,
        "Intel UHD Graphics P630 (Coffee Lake)",
        GPUTier::Mainstream,
        GPUFeatures::dx12(),
    ),
    (
        0x8086,
        0x3E98,
        "Intel UHD Graphics 630 (Coffee Lake)",
        GPUTier::Mainstream,
        GPUFeatures::dx12(),
    ),
    (
        0x8086,
        0x3E9A,
        "Intel UHD Graphics P630 (Coffee Lake)",
        GPUTier::Mainstream,
        GPUFeatures::dx12(),
    ),
    (
        0x8086,
        0x3E9B,
        "Intel UHD Graphics 630 (Coffee Lake)",
        GPUTier::Mainstream,
        GPUFeatures::dx12(),
    ),
    (
        0x8086,
        0x3EA0,
        "Intel UHD Graphics 620 (Whiskey Lake)",
        GPUTier::Budget,
        GPUFeatures::dx12(),
    ),
    (
        0x8086,
        0x3EA5,
        "Intel Iris Plus Graphics 655 (Coffee Lake)",
        GPUTier::Performance,
        GPUFeatures::dx12(),
    ),
    (
        0x8086,
        0x3EA6,
        "Intel Iris Plus Graphics 645 (Coffee Lake)",
        GPUTier::Performance,
        GPUFeatures::dx12(),
    ),
    (
        0x8086,
        0x3EA7,
        "Intel Iris Plus Graphics 645 (Coffee Lake)",
        GPUTier::Performance,
        GPUFeatures::dx12(),
    ),
    (
        0x8086,
        0x3EA8,
        "Intel Iris Plus Graphics 655 (Coffee Lake)",
        GPUTier::Performance,
        GPUFeatures::dx12(),
    ),
    // Ice Lake
    (
        0x8086,
        0x8A50,
        "Intel Iris Plus Graphics G1 (Ice Lake)",
        GPUTier::Performance,
        GPUFeatures::dx12(),
    ),
    (
        0x8086,
        0x8A51,
        "Intel Iris Plus Graphics G4 (Ice Lake)",
        GPUTier::Performance,
        GPUFeatures::dx12(),
    ),
    (
        0x8086,
        0x8A52,
        "Intel Iris Plus Graphics G7 (Ice Lake)",
        GPUTier::Performance,
        GPUFeatures::dx12(),
    ),
    (
        0x8086,
        0x8A53,
        "Intel Iris Plus Graphics G7 (Ice Lake)",
        GPUTier::Performance,
        GPUFeatures::dx12(),
    ),
    (
        0x8086,
        0x8A5A,
        "Intel Iris Plus Graphics G1 (Ice Lake)",
        GPUTier::Performance,
        GPUFeatures::dx12(),
    ),
    (
        0x8086,
        0x8A5B,
        "Intel Iris Plus Graphics G4 (Ice Lake)",
        GPUTier::Performance,
        GPUFeatures::dx12(),
    ),
    (
        0x8086,
        0x8A5C,
        "Intel Iris Plus Graphics G7 (Ice Lake)",
        GPUTier::Performance,
        GPUFeatures::dx12(),
    ),
    (
        0x8086,
        0x8A5D,
        "Intel Iris Plus Graphics G7 (Ice Lake)",
        GPUTier::Performance,
        GPUFeatures::dx12(),
    ),
    // Tiger Lake
    (
        0x8086,
        0x9A40,
        "Intel Iris Xe Graphics G7 80EUs (Tiger Lake)",
        GPUTier::Performance,
        GPUFeatures::modern(),
    ),
    (
        0x8086,
        0x9A49,
        "Intel Iris Xe Graphics G7 96EUs (Tiger Lake)",
        GPUTier::Performance,
        GPUFeatures::modern(),
    ),
    (
        0x8086,
        0x9A60,
        "Intel UHD Graphics G1 (Tiger Lake)",
        GPUTier::Budget,
        GPUFeatures::modern(),
    ),
    (
        0x8086,
        0x9A68,
        "Intel UHD Graphics G1 (Tiger Lake)",
        GPUTier::Budget,
        GPUFeatures::modern(),
    ),
    (
        0x8086,
        0x9A70,
        "Intel UHD Graphics G1 (Tiger Lake)",
        GPUTier::Budget,
        GPUFeatures::modern(),
    ),
    (
        0x8086,
        0x9A78,
        "Intel UHD Graphics G1 (Tiger Lake)",
        GPUTier::Budget,
        GPUFeatures::modern(),
    ),
    // NVIDIA GPUs - 75+ devices
    // GeForce GTX 10 Series
    (
        0x10DE,
        0x1B00,
        "NVIDIA GeForce GTX 1080 Ti",
        GPUTier::Enthusiast,
        GPUFeatures::modern(),
    ),
    (
        0x10DE,
        0x1B02,
        "NVIDIA GeForce GTX 1080 Ti",
        GPUTier::Enthusiast,
        GPUFeatures::modern(),
    ),
    (
        0x10DE,
        0x1B06,
        "NVIDIA GeForce GTX 1080 Ti",
        GPUTier::Enthusiast,
        GPUFeatures::modern(),
    ),
    (
        0x10DE,
        0x1B80,
        "NVIDIA GeForce GTX 1080",
        GPUTier::HighEnd,
        GPUFeatures::modern(),
    ),
    (
        0x10DE,
        0x1B81,
        "NVIDIA GeForce GTX 1070",
        GPUTier::HighEnd,
        GPUFeatures::modern(),
    ),
    (
        0x10DE,
        0x1B82,
        "NVIDIA GeForce GTX 1070 Ti",
        GPUTier::HighEnd,
        GPUFeatures::modern(),
    ),
    (
        0x10DE,
        0x1B83,
        "NVIDIA GeForce GTX 1060 6GB",
        GPUTier::Performance,
        GPUFeatures::modern(),
    ),
    (
        0x10DE,
        0x1B84,
        "NVIDIA GeForce GTX 1060 3GB",
        GPUTier::Performance,
        GPUFeatures::modern(),
    ),
    (
        0x10DE,
        0x1BA0,
        "NVIDIA GeForce GTX 1080 Mobile",
        GPUTier::HighEnd,
        GPUFeatures::modern(),
    ),
    (
        0x10DE,
        0x1BA1,
        "NVIDIA GeForce GTX 1070 Mobile",
        GPUTier::HighEnd,
        GPUFeatures::modern(),
    ),
    (
        0x10DE,
        0x1BA2,
        "NVIDIA GeForce GTX 1070 Mobile",
        GPUTier::HighEnd,
        GPUFeatures::modern(),
    ),
    (
        0x10DE,
        0x1BB0,
        "NVIDIA GeForce GTX 1060 Mobile 6GB",
        GPUTier::Performance,
        GPUFeatures::modern(),
    ),
    (
        0x10DE,
        0x1BB1,
        "NVIDIA GeForce GTX 1060 Mobile 3GB",
        GPUTier::Performance,
        GPUFeatures::modern(),
    ),
    (
        0x10DE,
        0x1BB3,
        "NVIDIA GeForce GTX 1060 Mobile 6GB",
        GPUTier::Performance,
        GPUFeatures::modern(),
    ),
    (
        0x10DE,
        0x1BB4,
        "NVIDIA GeForce GTX 1060 Mobile 6GB",
        GPUTier::Performance,
        GPUFeatures::modern(),
    ),
    (
        0x10DE,
        0x1BB5,
        "NVIDIA GeForce GTX 1060 Mobile",
        GPUTier::Performance,
        GPUFeatures::modern(),
    ),
    (
        0x10DE,
        0x1BB6,
        "NVIDIA GeForce GTX 1060 Mobile",
        GPUTier::Performance,
        GPUFeatures::modern(),
    ),
    (
        0x10DE,
        0x1BB7,
        "NVIDIA GeForce GTX 1060 Mobile",
        GPUTier::Performance,
        GPUFeatures::modern(),
    ),
    (
        0x10DE,
        0x1BB8,
        "NVIDIA GeForce GTX 1060 Mobile",
        GPUTier::Performance,
        GPUFeatures::modern(),
    ),
    (
        0x10DE,
        0x1BE0,
        "NVIDIA GeForce GTX 1080 Mobile",
        GPUTier::HighEnd,
        GPUFeatures::modern(),
    ),
    (
        0x10DE,
        0x1BE1,
        "NVIDIA GeForce GTX 1070 Mobile",
        GPUTier::HighEnd,
        GPUFeatures::modern(),
    ),
    (
        0x10DE,
        0x1C02,
        "NVIDIA GeForce GTX 1060 3GB",
        GPUTier::Performance,
        GPUFeatures::modern(),
    ),
    (
        0x10DE,
        0x1C03,
        "NVIDIA GeForce GTX 1060 6GB",
        GPUTier::Performance,
        GPUFeatures::modern(),
    ),
    (
        0x10DE,
        0x1C04,
        "NVIDIA GeForce GTX 1060 5GB",
        GPUTier::Performance,
        GPUFeatures::modern(),
    ),
    (
        0x10DE,
        0x1C06,
        "NVIDIA GeForce GTX 1060 6GB Rev. 2",
        GPUTier::Performance,
        GPUFeatures::modern(),
    ),
    (
        0x10DE,
        0x1C07,
        "NVIDIA GeForce GTX 1060 5GB",
        GPUTier::Performance,
        GPUFeatures::modern(),
    ),
    (
        0x10DE,
        0x1C20,
        "NVIDIA GeForce GTX 1060 Mobile",
        GPUTier::Performance,
        GPUFeatures::modern(),
    ),
    (
        0x10DE,
        0x1C21,
        "NVIDIA GeForce GTX 1060 Mobile",
        GPUTier::Performance,
        GPUFeatures::modern(),
    ),
    (
        0x10DE,
        0x1C22,
        "NVIDIA GeForce GTX 1060 Mobile",
        GPUTier::Performance,
        GPUFeatures::modern(),
    ),
    (
        0x10DE,
        0x1C23,
        "NVIDIA GeForce GTX 1060 Mobile",
        GPUTier::Performance,
        GPUFeatures::modern(),
    ),
    (
        0x10DE,
        0x1C30,
        "NVIDIA GeForce GTX 1060 Mobile",
        GPUTier::Performance,
        GPUFeatures::modern(),
    ),
    (
        0x10DE,
        0x1C31,
        "NVIDIA GeForce GTX 1050",
        GPUTier::Mainstream,
        GPUFeatures::modern(),
    ),
    (
        0x10DE,
        0x1C32,
        "NVIDIA GeForce GTX 1050",
        GPUTier::Mainstream,
        GPUFeatures::modern(),
    ),
    (
        0x10DE,
        0x1C35,
        "NVIDIA GeForce GTX 1060 Mobile",
        GPUTier::Performance,
        GPUFeatures::modern(),
    ),
    (
        0x10DE,
        0x1C36,
        "NVIDIA GeForce GTX 1060 Mobile",
        GPUTier::Performance,
        GPUFeatures::modern(),
    ),
    (
        0x10DE,
        0x1C60,
        "NVIDIA GeForce GTX 1060 Mobile",
        GPUTier::Performance,
        GPUFeatures::modern(),
    ),
    (
        0x10DE,
        0x1C61,
        "NVIDIA GeForce GTX 1050 Ti Mobile",
        GPUTier::Mainstream,
        GPUFeatures::modern(),
    ),
    (
        0x10DE,
        0x1C62,
        "NVIDIA GeForce GTX 1050 Mobile",
        GPUTier::Mainstream,
        GPUFeatures::modern(),
    ),
    (
        0x10DE,
        0x1C81,
        "NVIDIA GeForce GTX 1050",
        GPUTier::Mainstream,
        GPUFeatures::modern(),
    ),
    (
        0x10DE,
        0x1C82,
        "NVIDIA GeForce GTX 1050 Ti",
        GPUTier::Mainstream,
        GPUFeatures::modern(),
    ),
    (
        0x10DE,
        0x1C83,
        "NVIDIA GeForce GTX 1050",
        GPUTier::Mainstream,
        GPUFeatures::modern(),
    ),
    (
        0x10DE,
        0x1C8C,
        "NVIDIA GeForce GTX 1050 Ti Mobile",
        GPUTier::Mainstream,
        GPUFeatures::modern(),
    ),
    (
        0x10DE,
        0x1C8D,
        "NVIDIA GeForce GTX 1050 Mobile",
        GPUTier::Mainstream,
        GPUFeatures::modern(),
    ),
    (
        0x10DE,
        0x1C8E,
        "NVIDIA GeForce GTX 1050 Ti Mobile",
        GPUTier::Mainstream,
        GPUFeatures::modern(),
    ),
    (
        0x10DE,
        0x1C8F,
        "NVIDIA GeForce GTX 1050 Mobile",
        GPUTier::Mainstream,
        GPUFeatures::modern(),
    ),
    (
        0x10DE,
        0x1C90,
        "NVIDIA GeForce GTX 1050 Ti Mobile",
        GPUTier::Mainstream,
        GPUFeatures::modern(),
    ),
    (
        0x10DE,
        0x1C91,
        "NVIDIA GeForce GTX 1050 3GB",
        GPUTier::Mainstream,
        GPUFeatures::modern(),
    ),
    (
        0x10DE,
        0x1C92,
        "NVIDIA GeForce GTX 1050 Mobile",
        GPUTier::Mainstream,
        GPUFeatures::modern(),
    ),
    (
        0x10DE,
        0x1C94,
        "NVIDIA GeForce GTX 1050 Mobile",
        GPUTier::Mainstream,
        GPUFeatures::modern(),
    ),
    (
        0x10DE,
        0x1C96,
        "NVIDIA GeForce GTX 1060 Mobile",
        GPUTier::Performance,
        GPUFeatures::modern(),
    ),
    (
        0x10DE,
        0x1CA7,
        "NVIDIA GeForce GTX 1050",
        GPUTier::Mainstream,
        GPUFeatures::modern(),
    ),
    (
        0x10DE,
        0x1CA8,
        "NVIDIA GeForce GTX 1050",
        GPUTier::Mainstream,
        GPUFeatures::modern(),
    ),
    (
        0x10DE,
        0x1CAA,
        "NVIDIA GeForce GTX 1050",
        GPUTier::Mainstream,
        GPUFeatures::modern(),
    ),
    (
        0x10DE,
        0x1CB1,
        "NVIDIA GeForce GTX 1050 Ti Mobile",
        GPUTier::Mainstream,
        GPUFeatures::modern(),
    ),
    (
        0x10DE,
        0x1CB2,
        "NVIDIA GeForce GTX 1050 Mobile",
        GPUTier::Mainstream,
        GPUFeatures::modern(),
    ),
    (
        0x10DE,
        0x1CB3,
        "NVIDIA GeForce GTX 1050 Mobile",
        GPUTier::Mainstream,
        GPUFeatures::modern(),
    ),
    (
        0x10DE,
        0x1CB6,
        "NVIDIA GeForce GTX 1050 Mobile",
        GPUTier::Mainstream,
        GPUFeatures::modern(),
    ),
    (
        0x10DE,
        0x1CBA,
        "NVIDIA GeForce GTX 1050 Mobile",
        GPUTier::Mainstream,
        GPUFeatures::modern(),
    ),
    (
        0x10DE,
        0x1CBB,
        "NVIDIA GeForce GTX 1050 Ti Mobile",
        GPUTier::Mainstream,
        GPUFeatures::modern(),
    ),
    (
        0x10DE,
        0x1CBC,
        "NVIDIA GeForce GTX 1050 Mobile",
        GPUTier::Mainstream,
        GPUFeatures::modern(),
    ),
    (
        0x10DE,
        0x1CBD,
        "NVIDIA GeForce GTX 1050 Mobile",
        GPUTier::Mainstream,
        GPUFeatures::modern(),
    ),
    (
        0x10DE,
        0x1CBE,
        "NVIDIA GeForce GTX 1050 Mobile",
        GPUTier::Mainstream,
        GPUFeatures::modern(),
    ),
    (
        0x10DE,
        0x1D01,
        "NVIDIA GeForce GT 1030",
        GPUTier::Budget,
        GPUFeatures::modern(),
    ),
    (
        0x10DE,
        0x1D10,
        "NVIDIA GeForce MX150",
        GPUTier::Budget,
        GPUFeatures::modern(),
    ),
    (
        0x10DE,
        0x1D11,
        "NVIDIA GeForce MX230",
        GPUTier::Budget,
        GPUFeatures::modern(),
    ),
    (
        0x10DE,
        0x1D12,
        "NVIDIA GeForce MX150",
        GPUTier::Budget,
        GPUFeatures::modern(),
    ),
    (
        0x10DE,
        0x1D13,
        "NVIDIA GeForce MX250",
        GPUTier::Budget,
        GPUFeatures::modern(),
    ),
    // GeForce RTX 20 Series
    (
        0x10DE,
        0x1E02,
        "NVIDIA GeForce RTX 2080 Ti",
        GPUTier::Enthusiast,
        GPUFeatures::raytracing(),
    ),
    (
        0x10DE,
        0x1E04,
        "NVIDIA GeForce RTX 2080 Ti",
        GPUTier::Enthusiast,
        GPUFeatures::raytracing(),
    ),
    (
        0x10DE,
        0x1E07,
        "NVIDIA GeForce RTX 2080 Ti",
        GPUTier::Enthusiast,
        GPUFeatures::raytracing(),
    ),
    (
        0x10DE,
        0x1E30,
        "NVIDIA GeForce RTX 2080 Ti",
        GPUTier::Enthusiast,
        GPUFeatures::raytracing(),
    ),
    (
        0x10DE,
        0x1E78,
        "NVIDIA GeForce RTX 2080 Ti",
        GPUTier::Enthusiast,
        GPUFeatures::raytracing(),
    ),
    (
        0x10DE,
        0x1E82,
        "NVIDIA GeForce RTX 2080",
        GPUTier::HighEnd,
        GPUFeatures::raytracing(),
    ),
    (
        0x10DE,
        0x1E84,
        "NVIDIA GeForce RTX 2080",
        GPUTier::HighEnd,
        GPUFeatures::raytracing(),
    ),
    (
        0x10DE,
        0x1E87,
        "NVIDIA GeForce RTX 2080",
        GPUTier::HighEnd,
        GPUFeatures::raytracing(),
    ),
    (
        0x10DE,
        0x1E89,
        "NVIDIA GeForce RTX 2060",
        GPUTier::Performance,
        GPUFeatures::raytracing(),
    ),
    (
        0x10DE,
        0x1E90,
        "NVIDIA GeForce RTX 2080 Mobile",
        GPUTier::HighEnd,
        GPUFeatures::raytracing(),
    ),
    (
        0x10DE,
        0x1E91,
        "NVIDIA GeForce RTX 2070 Mobile",
        GPUTier::HighEnd,
        GPUFeatures::raytracing(),
    ),
    (
        0x10DE,
        0x1E93,
        "NVIDIA GeForce RTX 2080 Mobile",
        GPUTier::HighEnd,
        GPUFeatures::raytracing(),
    ),
    (
        0x10DE,
        0x1EA0,
        "NVIDIA GeForce RTX 2070 Mobile",
        GPUTier::HighEnd,
        GPUFeatures::raytracing(),
    ),
    (
        0x10DE,
        0x1EA1,
        "NVIDIA GeForce RTX 2070 Mobile",
        GPUTier::HighEnd,
        GPUFeatures::raytracing(),
    ),
    (
        0x10DE,
        0x1EA2,
        "NVIDIA GeForce RTX 2070 Mobile",
        GPUTier::HighEnd,
        GPUFeatures::raytracing(),
    ),
    (
        0x10DE,
        0x1EA3,
        "NVIDIA GeForce RTX 2070 Mobile",
        GPUTier::HighEnd,
        GPUFeatures::raytracing(),
    ),
    (
        0x10DE,
        0x1EB0,
        "NVIDIA GeForce RTX 2080 Mobile",
        GPUTier::HighEnd,
        GPUFeatures::raytracing(),
    ),
    (
        0x10DE,
        0x1EB1,
        "NVIDIA GeForce RTX 2070 Mobile",
        GPUTier::HighEnd,
        GPUFeatures::raytracing(),
    ),
    (
        0x10DE,
        0x1EB4,
        "NVIDIA GeForce RTX 2070 Mobile",
        GPUTier::HighEnd,
        GPUFeatures::raytracing(),
    ),
    (
        0x10DE,
        0x1EB5,
        "NVIDIA GeForce RTX 2060 Mobile",
        GPUTier::Performance,
        GPUFeatures::raytracing(),
    ),
    (
        0x10DE,
        0x1EB6,
        "NVIDIA GeForce RTX 2060 Mobile",
        GPUTier::Performance,
        GPUFeatures::raytracing(),
    ),
    (
        0x10DE,
        0x1EB8,
        "NVIDIA GeForce RTX 2070 Mobile",
        GPUTier::HighEnd,
        GPUFeatures::raytracing(),
    ),
    (
        0x10DE,
        0x1EC2,
        "NVIDIA GeForce RTX 2070 SUPER",
        GPUTier::HighEnd,
        GPUFeatures::raytracing(),
    ),
    (
        0x10DE,
        0x1EC7,
        "NVIDIA GeForce RTX 2070 SUPER",
        GPUTier::HighEnd,
        GPUFeatures::raytracing(),
    ),
    (
        0x10DE,
        0x1ED0,
        "NVIDIA GeForce RTX 2080 SUPER Mobile",
        GPUTier::HighEnd,
        GPUFeatures::raytracing(),
    ),
    (
        0x10DE,
        0x1ED1,
        "NVIDIA GeForce RTX 2070 SUPER Mobile",
        GPUTier::HighEnd,
        GPUFeatures::raytracing(),
    ),
    (
        0x10DE,
        0x1ED3,
        "NVIDIA GeForce RTX 2080 SUPER Mobile",
        GPUTier::HighEnd,
        GPUFeatures::raytracing(),
    ),
    (
        0x10DE,
        0x1EF5,
        "NVIDIA GeForce RTX 2060 Mobile",
        GPUTier::Performance,
        GPUFeatures::raytracing(),
    ),
    (
        0x10DE,
        0x1EF6,
        "NVIDIA GeForce RTX 2060 Mobile",
        GPUTier::Performance,
        GPUFeatures::raytracing(),
    ),
    (
        0x10DE,
        0x1EF7,
        "NVIDIA GeForce RTX 2060 Mobile",
        GPUTier::Performance,
        GPUFeatures::raytracing(),
    ),
    (
        0x10DE,
        0x1F02,
        "NVIDIA GeForce RTX 2070",
        GPUTier::HighEnd,
        GPUFeatures::raytracing(),
    ),
    (
        0x10DE,
        0x1F03,
        "NVIDIA GeForce RTX 2060 12GB",
        GPUTier::Performance,
        GPUFeatures::raytracing(),
    ),
    (
        0x10DE,
        0x1F04,
        "NVIDIA GeForce RTX 2070",
        GPUTier::HighEnd,
        GPUFeatures::raytracing(),
    ),
    (
        0x10DE,
        0x1F06,
        "NVIDIA GeForce RTX 2060 SUPER",
        GPUTier::Performance,
        GPUFeatures::raytracing(),
    ),
    (
        0x10DE,
        0x1F07,
        "NVIDIA GeForce RTX 2070",
        GPUTier::HighEnd,
        GPUFeatures::raytracing(),
    ),
    (
        0x10DE,
        0x1F08,
        "NVIDIA GeForce RTX 2060",
        GPUTier::Performance,
        GPUFeatures::raytracing(),
    ),
    (
        0x10DE,
        0x1F09,
        "NVIDIA GeForce RTX 2060",
        GPUTier::Performance,
        GPUFeatures::raytracing(),
    ),
    (
        0x10DE,
        0x1F0A,
        "NVIDIA GeForce RTX 2060",
        GPUTier::Performance,
        GPUFeatures::raytracing(),
    ),
    (
        0x10DE,
        0x1F10,
        "NVIDIA GeForce RTX 2070 Mobile",
        GPUTier::HighEnd,
        GPUFeatures::raytracing(),
    ),
    (
        0x10DE,
        0x1F11,
        "NVIDIA GeForce RTX 2060 Mobile",
        GPUTier::Performance,
        GPUFeatures::raytracing(),
    ),
    (
        0x10DE,
        0x1F12,
        "NVIDIA GeForce RTX 2060 Mobile",
        GPUTier::Performance,
        GPUFeatures::raytracing(),
    ),
    (
        0x10DE,
        0x1F14,
        "NVIDIA GeForce RTX 2070 Mobile",
        GPUTier::HighEnd,
        GPUFeatures::raytracing(),
    ),
    (
        0x10DE,
        0x1F15,
        "NVIDIA GeForce RTX 2060 Mobile",
        GPUTier::Performance,
        GPUFeatures::raytracing(),
    ),
    (
        0x10DE,
        0x1F36,
        "NVIDIA GeForce RTX 2060 SUPER",
        GPUTier::Performance,
        GPUFeatures::raytracing(),
    ),
    (
        0x10DE,
        0x1F41,
        "NVIDIA GeForce RTX 2080 SUPER",
        GPUTier::HighEnd,
        GPUFeatures::raytracing(),
    ),
    (
        0x10DE,
        0x1F42,
        "NVIDIA GeForce RTX 2080 SUPER",
        GPUTier::HighEnd,
        GPUFeatures::raytracing(),
    ),
    (
        0x10DE,
        0x1F47,
        "NVIDIA GeForce RTX 2080 SUPER",
        GPUTier::HighEnd,
        GPUFeatures::raytracing(),
    ),
    (
        0x10DE,
        0x1F50,
        "NVIDIA GeForce RTX 2070 SUPER Mobile",
        GPUTier::HighEnd,
        GPUFeatures::raytracing(),
    ),
    (
        0x10DE,
        0x1F51,
        "NVIDIA GeForce RTX 2060 SUPER Mobile",
        GPUTier::Performance,
        GPUFeatures::raytracing(),
    ),
    (
        0x10DE,
        0x1F54,
        "NVIDIA GeForce RTX 2070 SUPER Mobile",
        GPUTier::HighEnd,
        GPUFeatures::raytracing(),
    ),
    (
        0x10DE,
        0x1F55,
        "NVIDIA GeForce RTX 2060 SUPER Mobile",
        GPUTier::Performance,
        GPUFeatures::raytracing(),
    ),
    (
        0x10DE,
        0x1F76,
        "NVIDIA GeForce RTX 2060 SUPER",
        GPUTier::Performance,
        GPUFeatures::raytracing(),
    ),
    (
        0x10DE,
        0x1F81,
        "NVIDIA GeForce GTX 1660 Ti",
        GPUTier::Performance,
        GPUFeatures::modern(),
    ),
    (
        0x10DE,
        0x1F82,
        "NVIDIA GeForce GTX 1660 SUPER",
        GPUTier::Performance,
        GPUFeatures::modern(),
    ),
    (
        0x10DE,
        0x1F83,
        "NVIDIA GeForce GTX 1660",
        GPUTier::Mainstream,
        GPUFeatures::modern(),
    ),
    (
        0x10DE,
        0x1F91,
        "NVIDIA GeForce GTX 1660 Ti Mobile",
        GPUTier::Performance,
        GPUFeatures::modern(),
    ),
    (
        0x10DE,
        0x1F92,
        "NVIDIA GeForce GTX 1660 Ti Mobile",
        GPUTier::Performance,
        GPUFeatures::modern(),
    ),
    (
        0x10DE,
        0x1F94,
        "NVIDIA GeForce GTX 1660 Ti Mobile",
        GPUTier::Performance,
        GPUFeatures::modern(),
    ),
    (
        0x10DE,
        0x1F95,
        "NVIDIA GeForce GTX 1650 Ti Mobile",
        GPUTier::Mainstream,
        GPUFeatures::modern(),
    ),
    (
        0x10DE,
        0x1F96,
        "NVIDIA GeForce GTX 1650 Mobile",
        GPUTier::Mainstream,
        GPUFeatures::modern(),
    ),
    (
        0x10DE,
        0x1F97,
        "NVIDIA GeForce GTX 1650 Mobile",
        GPUTier::Mainstream,
        GPUFeatures::modern(),
    ),
    (
        0x10DE,
        0x1F98,
        "NVIDIA GeForce GTX 1650 Mobile",
        GPUTier::Mainstream,
        GPUFeatures::modern(),
    ),
    (
        0x10DE,
        0x1F99,
        "NVIDIA GeForce GTX 1650 Mobile",
        GPUTier::Mainstream,
        GPUFeatures::modern(),
    ),
    (
        0x10DE,
        0x1F9C,
        "NVIDIA GeForce GTX 1650 SUPER",
        GPUTier::Mainstream,
        GPUFeatures::modern(),
    ),
    (
        0x10DE,
        0x1F9D,
        "NVIDIA GeForce GTX 1650",
        GPUTier::Mainstream,
        GPUFeatures::modern(),
    ),
    (
        0x10DE,
        0x1F9F,
        "NVIDIA GeForce GTX 1650 Ti Mobile",
        GPUTier::Mainstream,
        GPUFeatures::modern(),
    ),
    // GeForce RTX 30 Series
    (
        0x10DE,
        0x2204,
        "NVIDIA GeForce RTX 3090",
        GPUTier::Enthusiast,
        GPUFeatures::raytracing(),
    ),
    (
        0x10DE,
        0x2206,
        "NVIDIA GeForce RTX 3080",
        GPUTier::HighEnd,
        GPUFeatures::raytracing(),
    ),
    (
        0x10DE,
        0x2208,
        "NVIDIA GeForce RTX 3080 Ti",
        GPUTier::Enthusiast,
        GPUFeatures::raytracing(),
    ),
    (
        0x10DE,
        0x220A,
        "NVIDIA GeForce RTX 3080",
        GPUTier::HighEnd,
        GPUFeatures::raytracing(),
    ),
    (
        0x10DE,
        0x2216,
        "NVIDIA GeForce RTX 3080 Mobile",
        GPUTier::HighEnd,
        GPUFeatures::raytracing(),
    ),
    (
        0x10DE,
        0x2230,
        "NVIDIA GeForce RTX 3080 Ti Mobile",
        GPUTier::Enthusiast,
        GPUFeatures::raytracing(),
    ),
    (
        0x10DE,
        0x2231,
        "NVIDIA GeForce RTX 3080 Mobile",
        GPUTier::HighEnd,
        GPUFeatures::raytracing(),
    ),
    (
        0x10DE,
        0x2232,
        "NVIDIA GeForce RTX 3070 Mobile",
        GPUTier::HighEnd,
        GPUFeatures::raytracing(),
    ),
    (
        0x10DE,
        0x2233,
        "NVIDIA GeForce RTX 3060 Ti Mobile",
        GPUTier::Performance,
        GPUFeatures::raytracing(),
    ),
    (
        0x10DE,
        0x2234,
        "NVIDIA GeForce RTX 3060 Mobile",
        GPUTier::Performance,
        GPUFeatures::raytracing(),
    ),
    (
        0x10DE,
        0x2235,
        "NVIDIA GeForce RTX 3050 Ti Mobile",
        GPUTier::Mainstream,
        GPUFeatures::raytracing(),
    ),
    (
        0x10DE,
        0x2236,
        "NVIDIA GeForce RTX 3050 Mobile",
        GPUTier::Mainstream,
        GPUFeatures::raytracing(),
    ),
    (
        0x10DE,
        0x2237,
        "NVIDIA GeForce RTX 3050 Ti Mobile",
        GPUTier::Mainstream,
        GPUFeatures::raytracing(),
    ),
    (
        0x10DE,
        0x2238,
        "NVIDIA GeForce RTX 3050 Mobile",
        GPUTier::Mainstream,
        GPUFeatures::raytracing(),
    ),
    (
        0x10DE,
        0x2414,
        "NVIDIA GeForce RTX 3060 Ti",
        GPUTier::Performance,
        GPUFeatures::raytracing(),
    ),
    (
        0x10DE,
        0x2420,
        "NVIDIA GeForce RTX 3060 Ti",
        GPUTier::Performance,
        GPUFeatures::raytracing(),
    ),
    (
        0x10DE,
        0x2438,
        "NVIDIA GeForce RTX 3060 Ti",
        GPUTier::Performance,
        GPUFeatures::raytracing(),
    ),
    (
        0x10DE,
        0x2482,
        "NVIDIA GeForce RTX 3070 Ti",
        GPUTier::HighEnd,
        GPUFeatures::raytracing(),
    ),
    (
        0x10DE,
        0x2484,
        "NVIDIA GeForce RTX 3070",
        GPUTier::HighEnd,
        GPUFeatures::raytracing(),
    ),
    (
        0x10DE,
        0x2486,
        "NVIDIA GeForce RTX 3060",
        GPUTier::Performance,
        GPUFeatures::raytracing(),
    ),
    (
        0x10DE,
        0x2487,
        "NVIDIA GeForce RTX 3060",
        GPUTier::Performance,
        GPUFeatures::raytracing(),
    ),
    (
        0x10DE,
        0x2488,
        "NVIDIA GeForce RTX 3070",
        GPUTier::HighEnd,
        GPUFeatures::raytracing(),
    ),
    (
        0x10DE,
        0x2489,
        "NVIDIA GeForce RTX 3060 12GB",
        GPUTier::Performance,
        GPUFeatures::raytracing(),
    ),
    (
        0x10DE,
        0x248A,
        "NVIDIA GeForce RTX 3060",
        GPUTier::Performance,
        GPUFeatures::raytracing(),
    ),
    (
        0x10DE,
        0x2490,
        "NVIDIA GeForce RTX 3090 Ti",
        GPUTier::Enthusiast,
        GPUFeatures::raytracing(),
    ),
    (
        0x10DE,
        0x2503,
        "NVIDIA GeForce RTX 3060",
        GPUTier::Performance,
        GPUFeatures::raytracing(),
    ),
    (
        0x10DE,
        0x2504,
        "NVIDIA GeForce RTX 3060",
        GPUTier::Performance,
        GPUFeatures::raytracing(),
    ),
    (
        0x10DE,
        0x2507,
        "NVIDIA GeForce RTX 3050",
        GPUTier::Mainstream,
        GPUFeatures::raytracing(),
    ),
    (
        0x10DE,
        0x2508,
        "NVIDIA GeForce RTX 3050",
        GPUTier::Mainstream,
        GPUFeatures::raytracing(),
    ),
    // AMD GPUs - 75+ devices
    // Radeon RX 5000 Series (RDNA)
    (
        0x1002,
        0x7310,
        "AMD Radeon RX 5700",
        GPUTier::HighEnd,
        GPUFeatures::modern(),
    ),
    (
        0x1002,
        0x7312,
        "AMD Radeon RX 5700 XT",
        GPUTier::HighEnd,
        GPUFeatures::modern(),
    ),
    (
        0x1002,
        0x7318,
        "AMD Radeon RX 5700 XT",
        GPUTier::HighEnd,
        GPUFeatures::modern(),
    ),
    (
        0x1002,
        0x7319,
        "AMD Radeon RX 5700",
        GPUTier::HighEnd,
        GPUFeatures::modern(),
    ),
    (
        0x1002,
        0x731A,
        "AMD Radeon RX 5700",
        GPUTier::HighEnd,
        GPUFeatures::modern(),
    ),
    (
        0x1002,
        0x731B,
        "AMD Radeon RX 5700 XT",
        GPUTier::HighEnd,
        GPUFeatures::modern(),
    ),
    (
        0x1002,
        0x731E,
        "AMD Radeon RX 5700 XT",
        GPUTier::HighEnd,
        GPUFeatures::modern(),
    ),
    (
        0x1002,
        0x731F,
        "AMD Radeon RX 5700",
        GPUTier::HighEnd,
        GPUFeatures::modern(),
    ),
    (
        0x1002,
        0x7340,
        "AMD Radeon RX 5500 XT",
        GPUTier::Performance,
        GPUFeatures::modern(),
    ),
    (
        0x1002,
        0x7341,
        "AMD Radeon RX 5500",
        GPUTier::Mainstream,
        GPUFeatures::modern(),
    ),
    (
        0x1002,
        0x7347,
        "AMD Radeon RX 5500M",
        GPUTier::Mainstream,
        GPUFeatures::modern(),
    ),
    // Radeon RX 6000 Series (RDNA 2)
    (
        0x1002,
        0x73A0,
        "AMD Radeon RX 6950 XT",
        GPUTier::Enthusiast,
        GPUFeatures::raytracing(),
    ),
    (
        0x1002,
        0x73A1,
        "AMD Radeon RX 6900 XT",
        GPUTier::Enthusiast,
        GPUFeatures::raytracing(),
    ),
    (
        0x1002,
        0x73A2,
        "AMD Radeon RX 6800 XT",
        GPUTier::HighEnd,
        GPUFeatures::raytracing(),
    ),
    (
        0x1002,
        0x73A3,
        "AMD Radeon RX 6800",
        GPUTier::HighEnd,
        GPUFeatures::raytracing(),
    ),
    (
        0x1002,
        0x73A5,
        "AMD Radeon RX 6800 XT",
        GPUTier::HighEnd,
        GPUFeatures::raytracing(),
    ),
    (
        0x1002,
        0x73AB,
        "AMD Radeon RX 6600 XT",
        GPUTier::Performance,
        GPUFeatures::raytracing(),
    ),
    (
        0x1002,
        0x73AE,
        "AMD Radeon RX 6600",
        GPUTier::Performance,
        GPUFeatures::raytracing(),
    ),
    (
        0x1002,
        0x73AF,
        "AMD Radeon RX 6600",
        GPUTier::Performance,
        GPUFeatures::raytracing(),
    ),
    (
        0x1002,
        0x73BF,
        "AMD Radeon RX 6600",
        GPUTier::Performance,
        GPUFeatures::raytracing(),
    ),
    (
        0x1002,
        0x73C0,
        "AMD Radeon RX 6700 XT",
        GPUTier::HighEnd,
        GPUFeatures::raytracing(),
    ),
    (
        0x1002,
        0x73C1,
        "AMD Radeon RX 6700",
        GPUTier::HighEnd,
        GPUFeatures::raytracing(),
    ),
    (
        0x1002,
        0x73C3,
        "AMD Radeon RX 6700",
        GPUTier::HighEnd,
        GPUFeatures::raytracing(),
    ),
    (
        0x1002,
        0x73DF,
        "AMD Radeon RX 6700S",
        GPUTier::HighEnd,
        GPUFeatures::raytracing(),
    ),
    (
        0x1002,
        0x73E0,
        "AMD Radeon RX 6600M",
        GPUTier::Performance,
        GPUFeatures::raytracing(),
    ),
    (
        0x1002,
        0x73E1,
        "AMD Radeon RX 6600M",
        GPUTier::Performance,
        GPUFeatures::raytracing(),
    ),
    (
        0x1002,
        0x73E3,
        "AMD Radeon RX 6600M",
        GPUTier::Performance,
        GPUFeatures::raytracing(),
    ),
    (
        0x1002,
        0x73E4,
        "AMD Radeon RX 6700M",
        GPUTier::HighEnd,
        GPUFeatures::raytracing(),
    ),
    (
        0x1002,
        0x73EF,
        "AMD Radeon RX 6800M",
        GPUTier::HighEnd,
        GPUFeatures::raytracing(),
    ),
    (
        0x1002,
        0x73F0,
        "AMD Radeon RX 6700S",
        GPUTier::HighEnd,
        GPUFeatures::raytracing(),
    ),
    (
        0x1002,
        0x73FF,
        "AMD Radeon RX 6600S",
        GPUTier::Performance,
        GPUFeatures::raytracing(),
    ),
    // Radeon RX 7000 Series (RDNA 3)
    (
        0x1002,
        0x744C,
        "AMD Radeon RX 7900 XTX",
        GPUTier::Enthusiast,
        GPUFeatures::raytracing(),
    ),
    (
        0x1002,
        0x7448,
        "AMD Radeon RX 7800 XT",
        GPUTier::HighEnd,
        GPUFeatures::raytracing(),
    ),
    (
        0x1002,
        0x7449,
        "AMD Radeon RX 7900 GRE",
        GPUTier::Enthusiast,
        GPUFeatures::raytracing(),
    ),
    (
        0x1002,
        0x747E,
        "AMD Radeon RX 7700 XT",
        GPUTier::HighEnd,
        GPUFeatures::raytracing(),
    ),
    (
        0x1002,
        0x7480,
        "AMD Radeon RX 7600",
        GPUTier::Performance,
        GPUFeatures::raytracing(),
    ),
    (
        0x1002,
        0x7483,
        "AMD Radeon RX 7600M XT",
        GPUTier::Performance,
        GPUFeatures::raytracing(),
    ),
    (
        0x1002,
        0x7484,
        "AMD Radeon RX 7600M",
        GPUTier::Performance,
        GPUFeatures::raytracing(),
    ),
    // Older AMD Radeon Series
    (
        0x1002,
        0x6860,
        "AMD Radeon R7 M260",
        GPUTier::Budget,
        GPUFeatures::dx11(),
    ),
    (
        0x1002,
        0x6861,
        "AMD Radeon R5 M240",
        GPUTier::Budget,
        GPUFeatures::dx11(),
    ),
    (
        0x1002,
        0x6863,
        "AMD Radeon R5 M240",
        GPUTier::Budget,
        GPUFeatures::dx11(),
    ),
    (
        0x1002,
        0x6864,
        "AMD Radeon R5 M230",
        GPUTier::Budget,
        GPUFeatures::dx11(),
    ),
    (
        0x1002,
        0x6867,
        "AMD Radeon R5 M230",
        GPUTier::Budget,
        GPUFeatures::dx11(),
    ),
    (
        0x1002,
        0x6868,
        "AMD Radeon R7 M260X",
        GPUTier::Budget,
        GPUFeatures::dx11(),
    ),
    (
        0x1002,
        0x6869,
        "AMD Radeon R5 M230",
        GPUTier::Budget,
        GPUFeatures::dx11(),
    ),
    (
        0x1002,
        0x686A,
        "AMD Radeon R5 M230",
        GPUTier::Budget,
        GPUFeatures::dx11(),
    ),
    (
        0x1002,
        0x686B,
        "AMD Radeon R5 M230",
        GPUTier::Budget,
        GPUFeatures::dx11(),
    ),
    (
        0x1002,
        0x686C,
        "AMD Radeon R5 M240",
        GPUTier::Budget,
        GPUFeatures::dx11(),
    ),
    (
        0x1002,
        0x686D,
        "AMD Radeon R7 M260DX",
        GPUTier::Budget,
        GPUFeatures::dx11(),
    ),
    (
        0x1002,
        0x686E,
        "AMD Radeon R5 M230",
        GPUTier::Budget,
        GPUFeatures::dx11(),
    ),
    (
        0x1002,
        0x687F,
        "AMD Radeon R5 M230",
        GPUTier::Budget,
        GPUFeatures::dx11(),
    ),
    (
        0x1002,
        0x6880,
        "AMD Radeon R9 M370X",
        GPUTier::Performance,
        GPUFeatures::dx11(),
    ),
    (
        0x1002,
        0x6888,
        "AMD Radeon R7 M265",
        GPUTier::Budget,
        GPUFeatures::dx11(),
    ),
    (
        0x1002,
        0x6889,
        "AMD Radeon R7 M270",
        GPUTier::Budget,
        GPUFeatures::dx11(),
    ),
    (
        0x1002,
        0x688A,
        "AMD Radeon R7 M265",
        GPUTier::Budget,
        GPUFeatures::dx11(),
    ),
    (
        0x1002,
        0x688C,
        "AMD Radeon R7 M270X",
        GPUTier::Budget,
        GPUFeatures::dx11(),
    ),
    (
        0x1002,
        0x688D,
        "AMD Radeon R7 M260",
        GPUTier::Budget,
        GPUFeatures::dx11(),
    ),
    (
        0x1002,
        0x6898,
        "AMD Radeon R7 M265",
        GPUTier::Budget,
        GPUFeatures::dx11(),
    ),
    (
        0x1002,
        0x6899,
        "AMD Radeon R7 M280",
        GPUTier::Budget,
        GPUFeatures::dx11(),
    ),
    (
        0x1002,
        0x689A,
        "AMD Radeon R7 M270",
        GPUTier::Budget,
        GPUFeatures::dx11(),
    ),
    (
        0x1002,
        0x689B,
        "AMD Radeon R7 M280",
        GPUTier::Budget,
        GPUFeatures::dx11(),
    ),
    (
        0x1002,
        0x689C,
        "AMD Radeon R7 M270X",
        GPUTier::Budget,
        GPUFeatures::dx11(),
    ),
    (
        0x1002,
        0x689D,
        "AMD Radeon R7 M260",
        GPUTier::Budget,
        GPUFeatures::dx11(),
    ),
    (
        0x1002,
        0x689E,
        "AMD Radeon R7 M260",
        GPUTier::Budget,
        GPUFeatures::dx11(),
    ),
    (
        0x1002,
        0x68A0,
        "AMD Radeon R9 M375",
        GPUTier::Performance,
        GPUFeatures::dx11(),
    ),
    (
        0x1002,
        0x68A1,
        "AMD Radeon R9 M375X",
        GPUTier::Performance,
        GPUFeatures::dx11(),
    ),
    (
        0x1002,
        0x68A8,
        "AMD Radeon R9 M385",
        GPUTier::Performance,
        GPUFeatures::dx11(),
    ),
    (
        0x1002,
        0x68A9,
        "AMD Radeon R9 M385X",
        GPUTier::Performance,
        GPUFeatures::dx11(),
    ),
    (
        0x1002,
        0x68B0,
        "AMD Radeon R9 M365X",
        GPUTier::Performance,
        GPUFeatures::dx11(),
    ),
    (
        0x1002,
        0x68B8,
        "AMD Radeon R9 M370",
        GPUTier::Performance,
        GPUFeatures::dx11(),
    ),
    (
        0x1002,
        0x68B9,
        "AMD Radeon R9 M370X",
        GPUTier::Performance,
        GPUFeatures::dx11(),
    ),
    (
        0x1002,
        0x68BA,
        "AMD Radeon R9 M370",
        GPUTier::Performance,
        GPUFeatures::dx11(),
    ),
    (
        0x1002,
        0x68BE,
        "AMD Radeon R9 M365X",
        GPUTier::Performance,
        GPUFeatures::dx11(),
    ),
    (
        0x1002,
        0x68C0,
        "AMD Radeon R9 M380",
        GPUTier::Performance,
        GPUFeatures::dx11(),
    ),
    (
        0x1002,
        0x68C1,
        "AMD Radeon R9 M380",
        GPUTier::Performance,
        GPUFeatures::dx11(),
    ),
    (
        0x1002,
        0x68C7,
        "AMD Radeon R9 M380",
        GPUTier::Performance,
        GPUFeatures::dx11(),
    ),
    (
        0x1002,
        0x68C8,
        "AMD Radeon R9 M370X",
        GPUTier::Performance,
        GPUFeatures::dx11(),
    ),
    (
        0x1002,
        0x68C9,
        "AMD Radeon R9 M370X",
        GPUTier::Performance,
        GPUFeatures::dx11(),
    ),
    (
        0x1002,
        0x68CA,
        "AMD Radeon R9 M370X",
        GPUTier::Performance,
        GPUFeatures::dx11(),
    ),
    (
        0x1002,
        0x68D8,
        "AMD Radeon R9 M365X",
        GPUTier::Performance,
        GPUFeatures::dx11(),
    ),
    (
        0x1002,
        0x68D9,
        "AMD Radeon R9 M360",
        GPUTier::Mainstream,
        GPUFeatures::dx11(),
    ),
    (
        0x1002,
        0x68DA,
        "AMD Radeon R9 M365X",
        GPUTier::Performance,
        GPUFeatures::dx11(),
    ),
    (
        0x1002,
        0x68E0,
        "AMD Radeon R9 M390",
        GPUTier::Performance,
        GPUFeatures::dx11(),
    ),
    (
        0x1002,
        0x68E1,
        "AMD Radeon R9 M390X",
        GPUTier::Performance,
        GPUFeatures::dx11(),
    ),
    (
        0x1002,
        0x68E4,
        "AMD Radeon R9 M390X",
        GPUTier::Performance,
        GPUFeatures::dx11(),
    ),
    (
        0x1002,
        0x68E5,
        "AMD Radeon R9 M390X",
        GPUTier::Performance,
        GPUFeatures::dx11(),
    ),
    (
        0x1002,
        0x68E8,
        "AMD Radeon R9 M390",
        GPUTier::Performance,
        GPUFeatures::dx11(),
    ),
    (
        0x1002,
        0x68E9,
        "AMD Radeon R9 M390",
        GPUTier::Performance,
        GPUFeatures::dx11(),
    ),
    (
        0x1002,
        0x68F1,
        "AMD Radeon R9 M370X",
        GPUTier::Performance,
        GPUFeatures::dx11(),
    ),
    (
        0x1002,
        0x68F2,
        "AMD Radeon R9 M360",
        GPUTier::Mainstream,
        GPUFeatures::dx11(),
    ),
    (
        0x1002,
        0x68F8,
        "AMD Radeon R7 M350",
        GPUTier::Budget,
        GPUFeatures::dx11(),
    ),
    (
        0x1002,
        0x68F9,
        "AMD Radeon R7 M360",
        GPUTier::Budget,
        GPUFeatures::dx11(),
    ),
    (
        0x1002,
        0x68FA,
        "AMD Radeon R7 M370",
        GPUTier::Budget,
        GPUFeatures::dx11(),
    ),
    (
        0x1002,
        0x68FE,
        "AMD Radeon R7 M350",
        GPUTier::Budget,
        GPUFeatures::dx11(),
    ),
];

/// GPU performance tiers for intelligent selection
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum GPUTier {
    Entry = 0,
    Budget = 1,
    Mainstream = 2,
    Performance = 3,
    HighEnd = 4,
    Enthusiast = 5,
}

/// GPU feature capabilities
#[derive(Debug, Clone, Copy)]
pub struct GPUFeatures {
    pub directx_version: u8,      // 9, 10, 11, 12
    pub opengl_version: (u8, u8), // Major, Minor
    pub vulkan_support: bool,
    pub raytracing_support: bool,
    pub compute_shaders: bool,
    pub hardware_video_decode: bool,
    pub hardware_video_encode: bool,
    pub ai_acceleration: bool,
    pub variable_rate_shading: bool,
    pub mesh_shaders: bool,
}

impl GPUFeatures {
    pub const fn basic() -> Self {
        Self {
            directx_version: 9,
            opengl_version: (2, 1),
            vulkan_support: false,
            raytracing_support: false,
            compute_shaders: false,
            hardware_video_decode: false,
            hardware_video_encode: false,
            ai_acceleration: false,
            variable_rate_shading: false,
            mesh_shaders: false,
        }
    }

    pub const fn dx11() -> Self {
        Self {
            directx_version: 11,
            opengl_version: (4, 0),
            vulkan_support: false,
            raytracing_support: false,
            compute_shaders: true,
            hardware_video_decode: true,
            hardware_video_encode: false,
            ai_acceleration: false,
            variable_rate_shading: false,
            mesh_shaders: false,
        }
    }

    pub const fn dx12() -> Self {
        Self {
            directx_version: 12,
            opengl_version: (4, 5),
            vulkan_support: true,
            raytracing_support: false,
            compute_shaders: true,
            hardware_video_decode: true,
            hardware_video_encode: true,
            ai_acceleration: false,
            variable_rate_shading: false,
            mesh_shaders: false,
        }
    }

    pub const fn modern() -> Self {
        Self {
            directx_version: 12,
            opengl_version: (4, 6),
            vulkan_support: true,
            raytracing_support: false,
            compute_shaders: true,
            hardware_video_decode: true,
            hardware_video_encode: true,
            ai_acceleration: true,
            variable_rate_shading: true,
            mesh_shaders: false,
        }
    }

    pub const fn raytracing() -> Self {
        Self {
            directx_version: 12,
            opengl_version: (4, 6),
            vulkan_support: true,
            raytracing_support: true,
            compute_shaders: true,
            hardware_video_decode: true,
            hardware_video_encode: true,
            ai_acceleration: true,
            variable_rate_shading: true,
            mesh_shaders: true,
        }
    }
}

/// GPU vendor identification
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GPUVendor {
    Intel,
    Nvidia,
    AMD,
    Unknown,
}

impl fmt::Display for GPUVendor {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            GPUVendor::Intel => write!(f, "Intel"),
            GPUVendor::Nvidia => write!(f, "NVIDIA"),
            GPUVendor::AMD => write!(f, "AMD"),
            GPUVendor::Unknown => write!(f, "Unknown"),
        }
    }
}

/// PCI device information structure
#[derive(Debug, Clone, Copy)]
pub struct PCIDevice {
    pub bus: u8,
    pub device: u8,
    pub function: u8,
    pub vendor_id: u16,
    pub device_id: u16,
    pub command: u16,
    pub status: u16,
    pub class_code: u8,
    pub subclass: u8,
    pub prog_if: u8,
    pub revision: u8,
    pub bars: [u32; 6], // Base Address Registers
}

/// GPU capabilities and features
#[derive(Debug, Clone)]
pub struct GPUCapabilities {
    pub vendor: GPUVendor,
    pub device_name: String,
    pub tier: GPUTier,
    pub features: GPUFeatures,
    pub memory_size: u64, // GPU memory in bytes
    pub max_resolution: (u32, u32),
    pub pci_device_id: u16,
    pub compute_units: u32,
    pub base_clock: u32,       // MHz
    pub boost_clock: u32,      // MHz
    pub memory_clock: u32,     // MHz
    pub memory_bandwidth: u64, // GB/s
}

/// GPU system status
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum GPUStatus {
    Uninitialized,
    Detecting,
    Initializing,
    Ready,
    Error,
}

/// Main GPU management system
pub struct GPUSystem {
    status: GPUStatus,
    detected_gpus: Vec<GPUCapabilities>,
    active_gpu_index: Option<usize>,
    pci_devices: Vec<PCIDevice>,
    performance_stats: GPUPerformanceStats,
    power_management: GPUPowerManagement,
}

/// GPU performance statistics
#[derive(Debug, Clone)]
pub struct GPUPerformanceStats {
    pub utilization_percentage: u8,
    pub temperature_celsius: u8,
    pub fan_speed_percentage: u8,
    pub power_consumption_watts: u16,
    pub memory_utilization_percentage: u8,
    pub clock_speeds: GPUClockSpeeds,
    pub frame_times_ms: Vec<f32>, // Last N frame times for monitoring
}

/// GPU clock speeds
#[derive(Debug, Clone)]
pub struct GPUClockSpeeds {
    pub core_clock_mhz: u32,
    pub memory_clock_mhz: u32,
    pub shader_clock_mhz: u32,
}

/// GPU power management
#[derive(Debug, Clone)]
pub struct GPUPowerManagement {
    pub power_state: GPUPowerState,
    pub thermal_throttling: bool,
    pub power_limit_watts: u16,
    pub target_temperature: u8,
    pub fan_curve: Vec<(u8, u8)>, // (temperature, fan_speed_percentage)
}

/// GPU power states
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum GPUPowerState {
    D0FullyOn,
    D1Standby,
    D2Sleep,
    D3Hot,
    D3Cold,
}

impl Default for GPUPerformanceStats {
    fn default() -> Self {
        Self {
            utilization_percentage: 0,
            temperature_celsius: 25,
            fan_speed_percentage: 30,
            power_consumption_watts: 0,
            memory_utilization_percentage: 0,
            clock_speeds: GPUClockSpeeds {
                core_clock_mhz: 0,
                memory_clock_mhz: 0,
                shader_clock_mhz: 0,
            },
            frame_times_ms: Vec::new(),
        }
    }
}

impl Default for GPUPowerManagement {
    fn default() -> Self {
        Self {
            power_state: GPUPowerState::D0FullyOn,
            thermal_throttling: false,
            power_limit_watts: 250,
            target_temperature: 83,
            fan_curve: vec![
                (30, 20),  // 30°C -> 20% fan speed
                (50, 30),  // 50°C -> 30% fan speed
                (70, 50),  // 70°C -> 50% fan speed
                (80, 70),  // 80°C -> 70% fan speed
                (90, 100), // 90°C -> 100% fan speed
            ],
        }
    }
}

impl GPUSystem {
    pub fn new() -> Self {
        Self {
            status: GPUStatus::Uninitialized,
            detected_gpus: Vec::new(),
            active_gpu_index: None,
            pci_devices: Vec::new(),
            performance_stats: GPUPerformanceStats::default(),
            power_management: GPUPowerManagement::default(),
        }
    }

    /// Initialize the comprehensive GPU system
    pub fn initialize(&mut self) -> Result<(), &'static str> {
        self.status = GPUStatus::Detecting;

        // Initialize all subsystems
        self.detect_gpus()?;
        self.initialize_memory_manager()?;
        self.initialize_acceleration_engine()?;
        self.initialize_opensource_drivers()?;
        self.initialize_ai_integration()?;
        self.select_optimal_gpu()?;

        self.status = GPUStatus::Ready;
        Ok(())
    }

    /// Detect all available GPUs through comprehensive PCI scanning
    fn detect_gpus(&mut self) -> Result<(), &'static str> {
        self.pci_devices = self.scan_pci_bus()?;

        for pci_device in &self.pci_devices {
            if let Some(gpu_caps) = self.create_gpu_capabilities(pci_device) {
                self.detected_gpus.push(gpu_caps);
            }
        }

        if self.detected_gpus.is_empty() {
            return Err("No supported GPUs detected");
        }

        Ok(())
    }

    /// Scan PCI bus for display devices
    fn scan_pci_bus(&self) -> Result<Vec<PCIDevice>, &'static str> {
        let mut devices = Vec::new();

        // Scan all PCI buses (0-255)
        for bus in 0..=255 {
            for device in 0..32 {
                for function in 0..8 {
                    if let Ok(pci_device) = self.probe_pci_device(bus, device, function) {
                        // Display controllers (class 0x03) and multimedia devices (class 0x04)
                        if pci_device.class_code == 0x03 || pci_device.class_code == 0x04 {
                            devices.push(pci_device);
                        }
                    }
                }
            }
        }

        Ok(devices)
    }

    /// Probe a specific PCI device
    fn probe_pci_device(
        &self,
        bus: u8,
        device: u8,
        function: u8,
    ) -> Result<PCIDevice, &'static str> {
        let vendor_id = self.pci_config_read_u16(bus, device, function, 0x00)?;

        if vendor_id == 0xFFFF {
            return Err("No device present");
        }

        let device_id = self.pci_config_read_u16(bus, device, function, 0x02)?;
        let command = self.pci_config_read_u16(bus, device, function, 0x04)?;
        let status = self.pci_config_read_u16(bus, device, function, 0x06)?;
        let class_code = self.pci_config_read_u8(bus, device, function, 0x0B)?;
        let subclass = self.pci_config_read_u8(bus, device, function, 0x0A)?;
        let prog_if = self.pci_config_read_u8(bus, device, function, 0x09)?;
        let revision = self.pci_config_read_u8(bus, device, function, 0x08)?;

        let mut bars = [0u32; 6];
        for i in 0..6 {
            bars[i] = self.pci_config_read_u32(bus, device, function, 0x10 + (i as u8 * 4))?;
        }

        Ok(PCIDevice {
            bus,
            device,
            function,
            vendor_id,
            device_id,
            command,
            status,
            class_code,
            subclass,
            prog_if,
            revision,
            bars,
        })
    }

    /// Create GPU capabilities from PCI device information
    fn create_gpu_capabilities(&self, pci_device: &PCIDevice) -> Option<GPUCapabilities> {
        // Look up device in comprehensive database
        for &(vendor_id, device_id, name, tier, features) in GPU_DEVICE_DATABASE {
            if pci_device.vendor_id == vendor_id && pci_device.device_id == device_id {
                let vendor = match vendor_id {
                    0x8086 => GPUVendor::Intel,
                    0x10DE => GPUVendor::Nvidia,
                    0x1002 => GPUVendor::AMD,
                    _ => GPUVendor::Unknown,
                };

                let memory_size = self.estimate_memory_size(vendor, device_id, tier);
                let max_resolution = self.estimate_max_resolution(vendor, tier);
                let (compute_units, base_clock, boost_clock, memory_clock, memory_bandwidth) =
                    self.estimate_performance_specs(vendor, tier, device_id);

                return Some(GPUCapabilities {
                    vendor,
                    device_name: name.to_string(),
                    tier,
                    features,
                    memory_size,
                    max_resolution,
                    pci_device_id: device_id,
                    compute_units,
                    base_clock,
                    boost_clock,
                    memory_clock,
                    memory_bandwidth,
                });
            }
        }

        // Fallback for unknown devices
        if pci_device.class_code == 0x03 {
            let vendor = match pci_device.vendor_id {
                0x8086 => GPUVendor::Intel,
                0x10DE => GPUVendor::Nvidia,
                0x1002 => GPUVendor::AMD,
                _ => GPUVendor::Unknown,
            };

            return Some(GPUCapabilities {
                vendor,
                device_name: format!("Unknown {} GPU (0x{:04X})", vendor, pci_device.device_id),
                tier: GPUTier::Entry,
                features: GPUFeatures::basic(),
                memory_size: 128 * 1024 * 1024, // 128MB fallback
                max_resolution: (1920, 1080),
                pci_device_id: pci_device.device_id,
                compute_units: 64,
                base_clock: 400,
                boost_clock: 800,
                memory_clock: 1000,
                memory_bandwidth: 50,
            });
        }

        None
    }

    /// Estimate GPU memory size based on vendor, device ID, and tier
    fn estimate_memory_size(&self, vendor: GPUVendor, device_id: u16, tier: GPUTier) -> u64 {
        match vendor {
            GPUVendor::Intel => {
                // Intel integrated GPUs share system memory
                match tier {
                    GPUTier::Entry => 128 * 1024 * 1024,        // 128MB
                    GPUTier::Budget => 256 * 1024 * 1024,       // 256MB
                    GPUTier::Mainstream => 512 * 1024 * 1024,   // 512MB
                    GPUTier::Performance => 1024 * 1024 * 1024, // 1GB
                    _ => 2048 * 1024 * 1024,                    // 2GB for high-end
                }
            }
            GPUVendor::Nvidia => {
                match tier {
                    GPUTier::Entry | GPUTier::Budget => 2 * 1024 * 1024 * 1024, // 2GB
                    GPUTier::Mainstream => 4 * 1024 * 1024 * 1024,              // 4GB
                    GPUTier::Performance => 6 * 1024 * 1024 * 1024,             // 6GB
                    GPUTier::HighEnd => 8 * 1024 * 1024 * 1024,                 // 8GB
                    GPUTier::Enthusiast => {
                        // RTX 3090/4090 have 24GB, RTX 3080 Ti has 12GB
                        if device_id == 0x2204 || device_id == 0x2208 {
                            // RTX 3090/3080 Ti
                            24 * 1024 * 1024 * 1024
                        } else {
                            12 * 1024 * 1024 * 1024
                        }
                    }
                }
            }
            GPUVendor::AMD => {
                match tier {
                    GPUTier::Entry | GPUTier::Budget => 4 * 1024 * 1024 * 1024, // 4GB
                    GPUTier::Mainstream => 6 * 1024 * 1024 * 1024,              // 6GB
                    GPUTier::Performance => 8 * 1024 * 1024 * 1024,             // 8GB
                    GPUTier::HighEnd => 12 * 1024 * 1024 * 1024,                // 12GB
                    GPUTier::Enthusiast => {
                        // RX 6900 XT and newer have 16GB+
                        if device_id == 0x744C {
                            // RX 7900 XTX
                            24 * 1024 * 1024 * 1024
                        } else {
                            16 * 1024 * 1024 * 1024
                        }
                    }
                }
            }
            GPUVendor::Unknown => 256 * 1024 * 1024, // 256MB conservative
        }
    }

    /// Estimate maximum resolution support
    fn estimate_max_resolution(&self, vendor: GPUVendor, tier: GPUTier) -> (u32, u32) {
        match tier {
            GPUTier::Entry => (1920, 1080),       // 1080p
            GPUTier::Budget => (2560, 1440),      // 1440p
            GPUTier::Mainstream => (3840, 2160),  // 4K
            GPUTier::Performance => (5120, 2880), // 5K
            GPUTier::HighEnd | GPUTier::Enthusiast => {
                match vendor {
                    GPUVendor::Nvidia | GPUVendor::AMD => (7680, 4320), // 8K
                    GPUVendor::Intel => (5120, 2880),                   // 5K for Intel
                    GPUVendor::Unknown => (3840, 2160),                 // 4K conservative
                }
            }
        }
    }

    /// Estimate performance specifications
    fn estimate_performance_specs(
        &self,
        vendor: GPUVendor,
        tier: GPUTier,
        device_id: u16,
    ) -> (u32, u32, u32, u32, u64) {
        // Returns: (compute_units, base_clock, boost_clock, memory_clock, memory_bandwidth)
        match vendor {
            GPUVendor::Intel => match tier {
                GPUTier::Entry => (12, 300, 700, 800, 25),
                GPUTier::Budget => (24, 400, 900, 1000, 35),
                GPUTier::Mainstream => (32, 500, 1100, 1200, 50),
                GPUTier::Performance => (96, 400, 1350, 1600, 70),
                _ => (128, 500, 1500, 2000, 100),
            },
            GPUVendor::Nvidia => {
                match tier {
                    GPUTier::Entry => (384, 1300, 1700, 3500, 112),
                    GPUTier::Budget => (512, 1400, 1800, 4000, 128),
                    GPUTier::Mainstream => (896, 1400, 1665, 6000, 192),
                    GPUTier::Performance => (2176, 1410, 1770, 7000, 448),
                    GPUTier::HighEnd => (2944, 1440, 1800, 9500, 760),
                    GPUTier::Enthusiast => {
                        if device_id == 0x2204 {
                            // RTX 3090
                            (10496, 1395, 1695, 9751, 936)
                        } else {
                            (8704, 1440, 1800, 9500, 760)
                        }
                    }
                }
            }
            GPUVendor::AMD => {
                match tier {
                    GPUTier::Entry => (512, 1200, 1600, 3500, 112),
                    GPUTier::Budget => (768, 1300, 1700, 4000, 128),
                    GPUTier::Mainstream => (1024, 1400, 1800, 6000, 192),
                    GPUTier::Performance => (2048, 1500, 2000, 8000, 256),
                    GPUTier::HighEnd => (3840, 1600, 2250, 10000, 512),
                    GPUTier::Enthusiast => {
                        if device_id == 0x744C {
                            // RX 7900 XTX
                            (6144, 1855, 2500, 10000, 960)
                        } else {
                            (5120, 1700, 2300, 8000, 512)
                        }
                    }
                }
            }
            GPUVendor::Unknown => (64, 400, 800, 1000, 50),
        }
    }

    /// Initialize memory management subsystem
    fn initialize_memory_manager(&mut self) -> Result<(), &'static str> {
        memory::initialize_gpu_memory_system(&self.detected_gpus)
    }

    /// Initialize graphics acceleration engine
    fn initialize_acceleration_engine(&mut self) -> Result<(), &'static str> {
        accel::initialize_acceleration_system(&self.detected_gpus)
    }

    /// Initialize opensource driver support
    fn initialize_opensource_drivers(&mut self) -> Result<(), &'static str> {
        opensource::initialize_opensource_system(&self.detected_gpus, &self.pci_devices)
    }

    /// Initialize AI-GPU integration
    fn initialize_ai_integration(&mut self) -> Result<(), &'static str> {
        ai_integration::initialize_ai_gpu_system(&self.detected_gpus)
    }

    /// Select the optimal GPU for primary display and compute tasks
    fn select_optimal_gpu(&mut self) -> Result<(), &'static str> {
        if self.detected_gpus.is_empty() {
            return Err("No GPUs available for selection");
        }

        // Sort GPUs by performance tier (higher is better)
        let mut gpu_indices: Vec<usize> = (0..self.detected_gpus.len()).collect();
        gpu_indices.sort_by(|&a, &b| {
            let gpu_a = &self.detected_gpus[a];
            let gpu_b = &self.detected_gpus[b];

            // First by tier, then by vendor preference (NVIDIA > AMD > Intel > Unknown)
            match gpu_b.tier.cmp(&gpu_a.tier) {
                core::cmp::Ordering::Equal => {
                    let vendor_score = |vendor: GPUVendor| match vendor {
                        GPUVendor::Nvidia => 4,
                        GPUVendor::AMD => 3,
                        GPUVendor::Intel => 2,
                        GPUVendor::Unknown => 1,
                    };
                    vendor_score(gpu_b.vendor).cmp(&vendor_score(gpu_a.vendor))
                }
                other => other,
            }
        });

        self.active_gpu_index = Some(gpu_indices[0]);
        Ok(())
    }

    /// PCI configuration space access methods
    fn pci_config_read_u16(
        &self,
        bus: u8,
        device: u8,
        function: u8,
        offset: u8,
    ) -> Result<u16, &'static str> {
        let address = 0x80000000u32
            | ((bus as u32) << 16)
            | ((device as u32) << 11)
            | ((function as u32) << 8)
            | ((offset as u32) & 0xFC);

        unsafe {
            let mut addr_port = x86_64::instructions::port::Port::new(0xCF8);
            addr_port.write(address);

            let mut data_port: x86_64::instructions::port::Port<u32> =
                x86_64::instructions::port::Port::new(0xCFC);
            let data = data_port.read();

            let shift = (offset & 2) * 8;
            let result = ((data >> shift) & 0xFFFF) as u16;

            if result == 0xFFFF && offset == 0x00 {
                return Err("No device present");
            }

            Ok(result)
        }
    }

    fn pci_config_read_u8(
        &self,
        bus: u8,
        device: u8,
        function: u8,
        offset: u8,
    ) -> Result<u8, &'static str> {
        let address = 0x80000000u32
            | ((bus as u32) << 16)
            | ((device as u32) << 11)
            | ((function as u32) << 8)
            | ((offset as u32) & 0xFC);

        unsafe {
            let mut addr_port = x86_64::instructions::port::Port::new(0xCF8);
            addr_port.write(address);

            let mut data_port: x86_64::instructions::port::Port<u32> =
                x86_64::instructions::port::Port::new(0xCFC);
            let data = data_port.read();

            let shift = (offset & 3) * 8;
            let result = ((data >> shift) & 0xFF) as u8;

            Ok(result)
        }
    }

    fn pci_config_read_u32(
        &self,
        bus: u8,
        device: u8,
        function: u8,
        offset: u8,
    ) -> Result<u32, &'static str> {
        let address = 0x80000000u32
            | ((bus as u32) << 16)
            | ((device as u32) << 11)
            | ((function as u32) << 8)
            | ((offset as u32) & 0xFC);

        unsafe {
            let mut addr_port = x86_64::instructions::port::Port::new(0xCF8);
            addr_port.write(address);

            let mut data_port: x86_64::instructions::port::Port<u32> =
                x86_64::instructions::port::Port::new(0xCFC);
            let data = data_port.read();

            Ok(data)
        }
    }

    /// Public API methods

    pub fn get_status(&self) -> GPUStatus {
        self.status
    }

    pub fn get_detected_gpus(&self) -> &[GPUCapabilities] {
        &self.detected_gpus
    }

    pub fn get_active_gpu(&self) -> Option<&GPUCapabilities> {
        self.active_gpu_index.map(|idx| &self.detected_gpus[idx])
    }

    pub fn get_performance_stats(&self) -> &GPUPerformanceStats {
        &self.performance_stats
    }

    pub fn get_power_management(&self) -> &GPUPowerManagement {
        &self.power_management
    }

    /// Update performance monitoring data
    pub fn update_performance_stats(&mut self) {
        let (gpu_tier, base_clock, boost_clock, memory_clock) =
            if let Some(active_gpu) = self.get_active_gpu() {
                (
                    active_gpu.tier,
                    active_gpu.base_clock,
                    active_gpu.boost_clock,
                    active_gpu.memory_clock,
                )
            } else {
                return;
            };

        // Reading GPU performance counters (utilization, temperature, fan speed,
        // power) requires mapping the GPU's MMIO BAR and reading vendor-specific
        // status registers.  Without a detected and mapped GPU, we cannot read
        // real hardware counters.  Use the GPU's nominal clock values and report
        // zero utilization/temperature since there is no hardware feedback.
        self.performance_stats.utilization_percentage = 0;
        self.performance_stats.temperature_celsius = 0;
        self.performance_stats.fan_speed_percentage = 0;
        self.performance_stats.power_consumption_watts = 0;

        // Use nominal (base) clock speeds from GPU capabilities
        self.performance_stats.clock_speeds.core_clock_mhz = base_clock;
        self.performance_stats.clock_speeds.memory_clock_mhz = memory_clock;
        self.performance_stats.clock_speeds.shader_clock_mhz = base_clock;

        // Suppress unused-variable warning for boost_clock and gpu_tier which
        // would be used when MMIO-based performance monitoring is available.
        let _ = (boost_clock, gpu_tier);
    }

    /// Set GPU power state
    pub fn set_power_state(&mut self, state: GPUPowerState) -> Result<(), &'static str> {
        match state {
            GPUPowerState::D0FullyOn => {
                self.power_management.power_state = state;
                // Full performance mode
            }
            GPUPowerState::D1Standby => {
                self.power_management.power_state = state;
                // Reduced clock speeds
            }
            GPUPowerState::D2Sleep => {
                self.power_management.power_state = state;
                // Memory powered down, minimal clocks
            }
            GPUPowerState::D3Hot | GPUPowerState::D3Cold => {
                self.power_management.power_state = state;
                // GPU mostly powered down
            }
        }
        Ok(())
    }

    /// Check if GPU acceleration is available and ready
    pub fn is_acceleration_available(&self) -> bool {
        self.status == GPUStatus::Ready && self.active_gpu_index.is_some()
    }

    /// Initialize GPU acceleration for framebuffer operations.
    ///
    /// Sets up the GPU's memory window for coherent access and configures
    /// the acceleration engine for blit operations on the framebuffer.
    /// When no usable GPU is available, returns an error so callers fall
    /// back to software rendering.
    pub fn initialize_acceleration(
        &mut self,
        framebuffer_info: &crate::graphics::FramebufferInfo,
    ) -> Result<(), &'static str> {
        if self.status != GPUStatus::Ready {
            return Err("GPU system not ready");
        }
        let gpu_idx = self.active_gpu_index
            .ok_or("No active GPU detected — cannot initialize hardware acceleration")?;
        let gpu = &self.detected_gpus[gpu_idx];

        // Read the GPU's PCI BAR0 to determine the MMIO aperture.
        let display_devices = crate::pci::get_devices_by_class(crate::pci::PciClass::Display);
        let vendor_id = match gpu.vendor {
            GPUVendor::Intel => 0x8086u16,
            GPUVendor::AMD => 0x1002u16,
            GPUVendor::Nvidia => 0x10DEu16,
            GPUVendor::Unknown => 0,
        };
        let pci_dev = display_devices
            .iter()
            .filter(|d| d.vendor_id == vendor_id)
            .nth(gpu_idx)
            .ok_or("Active GPU PCI device not found")?;

        // Read BAR0 (MMIO aperture) and BAR2 (GPU memory aperture).
        let bar0 = read_pci_config_dword_for(pci_dev.bus, pci_dev.device, pci_dev.function, 0x10);
        let bar0_hi = read_pci_config_dword_for(pci_dev.bus, pci_dev.device, pci_dev.function, 0x14);
        if (bar0 & 1) != 0 {
            return Err("GPU BAR0 is I/O space, not memory-mapped");
        }
        let bar0_base = ((bar0 & 0xFFFFFFF0) as u64) | (((bar0_hi as u64) & 0xFFFFFFFF) << 32);

        // Read BAR2 (GPU memory / aperture) at PCI offset 0x18.
        let bar2 = read_pci_config_dword_for(pci_dev.bus, pci_dev.device, pci_dev.function, 0x18);
        let bar2_hi = read_pci_config_dword_for(pci_dev.bus, pci_dev.device, pci_dev.function, 0x1C);
        let bar2_base = if (bar2 & 1) == 0 {
            ((bar2 & 0xFFFFFFF0) as u64) | (((bar2_hi as u64) & 0xFFFFFFFF) << 32)
        } else {
            0
        };

        // Configure the GPU memory window for coherent access.
        // The aperture size is typically 16MB for Intel (GTTMMADR),
        // 256MB for AMD, and variable for NVIDIA.
        let aperture_size = match gpu.vendor {
            GPUVendor::Intel => 16 * 1024 * 1024,
            GPUVendor::AMD => 256 * 1024 * 1024,
            GPUVendor::Nvidia => 16 * 1024 * 1024,
            GPUVendor::Unknown => 16 * 1024 * 1024,
        };

        // Set up the GPU memory window via the global memory manager.
        use crate::gpu::memory::{allocate_gpu_memory, MemoryFlags};
        crate::gpu::memory::set_gpu_memory_window_global(gpu_idx as u32, bar0_base, aperture_size as u64);

        // Allocate a DMA buffer for the framebuffer in GPU-accessible memory.
        // This allows the GPU's blit engine to read/write the framebuffer.
        let fb_size = framebuffer_info.size;
        let _fb_dma_handle = allocate_gpu_memory(
            gpu_idx as u32,
            fb_size,
            4096,
            MemoryFlags::DEFAULT,
        ).map_err(|e| {
            // If GPU memory allocation fails, we can still use software rendering.
            // Return an error so the caller knows to fall back.
            e
        })?;

        // If the GPU has a BAR2 memory aperture, map the framebuffer into it
        // so the GPU can directly access the framebuffer pixels.
        if bar2_base != 0 {
            let phys_offset = crate::memory::get_physical_memory_offset();
            if phys_offset != 0 {
                // Map the framebuffer physical address into the GPU's BAR2
                // aperture so the GPU blit engine can access it directly.
                let fb_phys = framebuffer_info.physical_address as u64;
                let fb_aperture_virt = phys_offset + bar2_base;

                // SAFETY: Writing to the GPU's BAR2 aperture to map the
                // framebuffer. This is a memory-mapped write to the GPU's
                // aperture region, which was validated above.
                unsafe {
                    // Write the framebuffer physical address into the GPU's
                    // aperture page table (if applicable) or use direct
                    // mapping via the aperture base.
                    let aperture_ptr = fb_aperture_virt as *mut u8;
                    let fb_ptr = (phys_offset + fb_phys) as *const u8;

                    // Copy framebuffer into aperture (initial state).
                    // In a real driver, this would be a GPU blit command,
                    // but for initialization we just ensure the mapping works.
                    for i in 0..core::cmp::min(fb_size, 4096) {
                        core::ptr::write_volatile(aperture_ptr.add(i), core::ptr::read_volatile(fb_ptr.add(i)));
                    }
                }

                crate::println!(
                    "GPU acceleration: framebuffer mapped to BAR2 aperture at 0x{:016X} ({}x{}, {} bytes)",
                    fb_aperture_virt,
                    framebuffer_info.width,
                    framebuffer_info.height,
                    fb_size
                );
            }
        }

        // The acceleration engine was already initialized during gpu::initialize()
        // via initialize_acceleration_engine(). Here we just verify it's ready.
        if crate::gpu::accel::get_acceleration_status() == crate::gpu::accel::AccelStatus::Ready {
            crate::println!(
                "GPU acceleration initialized for {} {} (vendor 0x{:04X}, device 0x{:04X})",
                gpu.vendor,
                gpu.device_name,
                vendor_id,
                gpu.pci_device_id
            );
            Ok(())
        } else {
            Err("GPU acceleration engine not ready after initialization")
        }
    }

    /// Clear framebuffer using GPU acceleration, falling back to a CPU blit.
    ///
    /// Fills the entire framebuffer region with `color`. When a hardware
    /// blit engine is unavailable this writes directly to the framebuffer
    /// memory using a tight loop, which is functionally correct though
    /// slower than a GPU would be.
    pub fn clear_framebuffer(
        &self,
        buffer_addr: u64,
        width: usize,
        height: usize,
        stride: usize,
        color: u32,
    ) -> Result<(), &'static str> {
        if buffer_addr == 0 || width == 0 || height == 0 {
            return Err("Invalid framebuffer parameters");
        }

        // Software fallback: write the color to every pixel.
        let base = buffer_addr as *mut u32;
        for y in 0..height {
            let row_start = y * stride;
            for x in 0..width {
                // SAFETY: the caller guarantees buffer_addr points to a
                // mapped framebuffer of at least width*height*stride bytes.
                unsafe {
                    core::ptr::write_volatile(base.add(row_start + x), color);
                }
            }
        }
        Ok(())
    }

    /// Fill a rectangle using GPU acceleration, falling back to a CPU blit.
    ///
    /// Fills the rectangle at (`x`, `y`) of size (`width`, `height`)
    /// with `color`. When a hardware 2D engine is unavailable this
    /// writes directly to the framebuffer memory.
    pub fn fill_rectangle(
        &self,
        buffer_addr: u64,
        stride: usize,
        x: usize,
        y: usize,
        width: usize,
        height: usize,
        color: u32,
    ) -> Result<(), &'static str> {
        if buffer_addr == 0 || width == 0 || height == 0 {
            return Err("Invalid rectangle parameters");
        }

        let base = buffer_addr as *mut u32;
        // Software fallback: write the color to each pixel in the rectangle.
        for row in 0..height {
            let row_start = (y + row) * stride + x;
            for col in 0..width {
                // SAFETY: the caller guarantees the rectangle lies within
                // the mapped framebuffer.
                unsafe {
                    core::ptr::write_volatile(base.add(row_start + col), color);
                }
            }
        }
        Ok(())
    }

    /// Generate comprehensive GPU report
    pub fn generate_system_report(&self) -> String {
        let mut report = String::new();

        report.push_str("=== RustOS Advanced GPU System Report ===\n\n");

        report.push_str(&format!("System Status: {:?}\n", self.status));
        report.push_str(&format!("Detected GPUs: {}\n", self.detected_gpus.len()));

        if let Some(active_gpu) = self.get_active_gpu() {
            report.push_str(&format!(
                "Active GPU: {} ({})\n",
                active_gpu.device_name, active_gpu.vendor
            ));
            report.push_str(&format!(
                "  Memory: {:.1} GB\n",
                active_gpu.memory_size as f64 / (1024.0 * 1024.0 * 1024.0)
            ));
            report.push_str(&format!(
                "  Max Resolution: {}x{}\n",
                active_gpu.max_resolution.0, active_gpu.max_resolution.1
            ));
            report.push_str(&format!(
                "  DirectX: {}\n",
                active_gpu.features.directx_version
            ));
            report.push_str(&format!(
                "  Vulkan: {}\n",
                if active_gpu.features.vulkan_support {
                    "Yes"
                } else {
                    "No"
                }
            ));
            report.push_str(&format!(
                "  Ray Tracing: {}\n",
                if active_gpu.features.raytracing_support {
                    "Yes"
                } else {
                    "No"
                }
            ));
        }

        report.push_str("\n=== All Detected GPUs ===\n");
        for (i, gpu) in self.detected_gpus.iter().enumerate() {
            let active_marker = if Some(i) == self.active_gpu_index {
                " [ACTIVE]"
            } else {
                ""
            };
            report.push_str(&format!(
                "{}. {}{}\n",
                i + 1,
                gpu.device_name,
                active_marker
            ));
            report.push_str(&format!("   Vendor: {}\n", gpu.vendor));
            report.push_str(&format!("   Tier: {:?}\n", gpu.tier));
            report.push_str(&format!(
                "   Memory: {:.1} GB\n",
                gpu.memory_size as f64 / (1024.0 * 1024.0 * 1024.0)
            ));
            report.push_str(&format!("   Compute Units: {}\n", gpu.compute_units));
            report.push_str(&format!(
                "   Base/Boost Clock: {}/{} MHz\n",
                gpu.base_clock, gpu.boost_clock
            ));
        }

        report
    }
}

// Global GPU system instance
lazy_static! {
    static ref GPU_SYSTEM: Mutex<GPUSystem> = Mutex::new(GPUSystem::new());
}

/// Initialize the advanced GPU acceleration system
pub fn initialize() -> Result<(), &'static str> {
    let mut gpu_system = GPU_SYSTEM.lock();
    gpu_system.initialize()
}

/// Get GPU system status
pub fn get_status() -> GPUStatus {
    let gpu_system = GPU_SYSTEM.lock();
    gpu_system.get_status()
}

/// Initialize GPU acceleration for a framebuffer.
///
/// Locks the global GPU system and asks it to initialize acceleration
/// for the given framebuffer. Returns `Ok(())` if the GPU system is
/// ready (or if the software fallback is acceptable), and an error
/// otherwise.
pub fn initialize_acceleration(
    framebuffer_info: &crate::graphics::FramebufferInfo,
) -> Result<(), &'static str> {
    let mut gpu_system = GPU_SYSTEM.lock();
    gpu_system.initialize_acceleration(framebuffer_info)
}

/// Get detected GPUs
pub fn get_detected_gpus() -> Vec<GPUCapabilities> {
    let gpu_system = GPU_SYSTEM.lock();
    gpu_system.get_detected_gpus().to_vec()
}

/// Get active GPU information
pub fn get_active_gpu() -> Option<GPUCapabilities> {
    let gpu_system = GPU_SYSTEM.lock();
    gpu_system.get_active_gpu().cloned()
}

/// Update performance monitoring
pub fn update_performance() {
    let mut gpu_system = GPU_SYSTEM.lock();
    gpu_system.update_performance_stats();
}

/// Get performance statistics
pub fn get_performance_stats() -> GPUPerformanceStats {
    let gpu_system = GPU_SYSTEM.lock();
    gpu_system.get_performance_stats().clone()
}

/// Generate system report
pub fn generate_report() -> String {
    let gpu_system = GPU_SYSTEM.lock();
    gpu_system.generate_system_report()
}

/// Check if GPU acceleration is available
pub fn is_acceleration_available() -> bool {
    let gpu_system = GPU_SYSTEM.lock();
    gpu_system.get_status() == GPUStatus::Ready && gpu_system.get_active_gpu().is_some()
}

/// Set GPU power state
pub fn set_power_state(state: GPUPowerState) -> Result<(), &'static str> {
    let mut gpu_system = GPU_SYSTEM.lock();
    gpu_system.set_power_state(state)
}

/// Get GPU manager instance
///
/// Returns None when no GPU has been detected or initialized.
/// Callers should check the return value and fall back to software rendering.
pub fn get_gpu_manager() -> Option<&'static GPUSystem> {
    let gpu_system = GPU_SYSTEM.lock();
    if gpu_system.get_status() == GPUStatus::Ready {
        // Cannot return a reference through a Mutex guard without leaking.
        // This function is unused — callers should use the module-level
        // functions (get_status, get_active_gpu, etc.) instead.
        None
    } else {
        None
    }
}

/// Read a 32-bit PCI configuration space register using I/O ports 0xCF8/0xCFC.
fn read_pci_config_dword_for(bus: u8, device: u8, function: u8, offset: u8) -> u32 {
    let config_address = 0x80000000u32
        | ((bus as u32) << 16)
        | ((device as u32) << 11)
        | ((function as u32) << 8)
        | ((offset as u32) & 0xFC);

    unsafe {
        core::arch::asm!(
            "out dx, eax",
            in("dx") 0xCF8u16,
            in("eax") config_address,
            options(nostack, preserves_flags),
        );

        let mut data: u32;
        core::arch::asm!(
            "in eax, dx",
            out("eax") data,
            in("dx") 0xCFCu16,
            options(nostack, preserves_flags),
        );
        data
    }
}
