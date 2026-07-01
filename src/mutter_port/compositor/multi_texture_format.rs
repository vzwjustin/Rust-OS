//! Multi-plane pixel formats for video/compositor buffers.
//! Ported from GNOME Mutter's src/compositor/meta-multi-texture-format.c
//!
//! Describes YUV and packed formats: plane counts, subsampling, bytes-per-pixel,
//! and texture unit mapping for shader sampling.

/// Format metadata: plane count, subsampling ratios
#[derive(Debug, Clone, Copy)]
pub struct FormatInfo {
    pub n_planes: u8,
    pub hsub: [u8; 3],
    pub vsub: [u8; 3],
}

/// Multi-plane pixel formats
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum MultiTextureFormat {
    Invalid = 0,
    Simple = 1,
    Yuyv = 2,
    Yvyu = 3,
    Uyvy = 4,
    Vyuy = 5,
    Nv12 = 6,
    Nv21 = 7,
    Nv16 = 8,
    Nv61 = 9,
    Nv24 = 10,
    Nv42 = 11,
    P010 = 12,
    P012 = 13,
    P016 = 14,
    Yuv420 = 15,
    Yvu420 = 16,
    Yuv422 = 17,
    Yvu422 = 18,
    Yuv444 = 19,
    Yvu444 = 20,
    S010 = 21,
    S210 = 22,
    S410 = 23,
    S012 = 24,
    S212 = 25,
    S412 = 26,
    S016 = 27,
    S216 = 28,
    S416 = 29,
}

impl MultiTextureFormat {
    /// Returns format name
    pub fn name(&self) -> &'static str {
        match self {
            Self::Invalid => "",
            Self::Simple => "",
            Self::Yuyv => "YUYV",
            Self::Yvyu => "YVYU",
            Self::Uyvy => "UYVY",
            Self::Vyuy => "VYUY",
            Self::Nv12 => "NV12",
            Self::Nv21 => "NV21",
            Self::Nv16 => "NV16",
            Self::Nv61 => "NV61",
            Self::Nv24 => "NV24",
            Self::Nv42 => "NV42",
            Self::P010 => "P010",
            Self::P012 => "P012",
            Self::P016 => "P016",
            Self::Yuv420 => "YUV420",
            Self::Yvu420 => "YVU420",
            Self::Yuv422 => "YUV422",
            Self::Yvu422 => "YVU422",
            Self::Yuv444 => "YUV444",
            Self::Yvu444 => "YVU444",
            Self::S010 => "S010",
            Self::S210 => "S210",
            Self::S410 => "S410",
            Self::S012 => "S012",
            Self::S212 => "S212",
            Self::S412 => "S412",
            Self::S016 => "S016",
            Self::S216 => "S216",
            Self::S416 => "S416",
        }
    }

    /// Returns format info (plane count, subsampling)
    pub fn info(&self) -> FormatInfo {
        match self {
            Self::Invalid => FormatInfo {
                n_planes: 0,
                hsub: [0, 0, 0],
                vsub: [0, 0, 0],
            },
            Self::Simple => FormatInfo {
                n_planes: 1,
                hsub: [1, 0, 0],
                vsub: [1, 0, 0],
            },
            Self::Yuyv | Self::Yvyu => FormatInfo {
                n_planes: 2,
                hsub: [1, 2, 0],
                vsub: [1, 1, 0],
            },
            Self::Uyvy | Self::Vyuy => FormatInfo {
                n_planes: 2,
                hsub: [2, 1, 0],
                vsub: [1, 1, 0],
            },
            Self::Nv12 | Self::Nv21 | Self::P010 | Self::P012 | Self::P016 => FormatInfo {
                n_planes: 2,
                hsub: [1, 2, 0],
                vsub: [1, 2, 0],
            },
            Self::Nv16 | Self::Nv61 => FormatInfo {
                n_planes: 2,
                hsub: [1, 2, 0],
                vsub: [1, 1, 0],
            },
            Self::Nv24 | Self::Nv42 => FormatInfo {
                n_planes: 2,
                hsub: [1, 1, 0],
                vsub: [1, 1, 0],
            },
            Self::Yuv420 | Self::Yvu420 | Self::S010 | Self::S012 | Self::S016 => FormatInfo {
                n_planes: 3,
                hsub: [1, 2, 2],
                vsub: [1, 2, 2],
            },
            Self::Yuv422 | Self::Yvu422 | Self::S210 | Self::S212 | Self::S216 => FormatInfo {
                n_planes: 3,
                hsub: [1, 2, 2],
                vsub: [1, 1, 1],
            },
            Self::Yuv444 | Self::Yvu444 | Self::S410 | Self::S412 | Self::S416 => FormatInfo {
                n_planes: 3,
                hsub: [1, 1, 1],
                vsub: [1, 1, 1],
            },
        }
    }
}
