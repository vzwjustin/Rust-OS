//! Port of GNOME mutter's `clutter/clutter-color-state.{c,h}` and
//! `clutter-color-state-private.h`, plus the pure data/logic parts of the
//! `ClutterColorState` subclasses `clutter-color-state-params.{c,h}` (the
//! `ClutterColorspace`/`ClutterTransferFunction`/`ClutterPrimaries`/
//! `ClutterColorimetry`/`ClutterEOTF`/`ClutterLuminance` value types and
//! their equality and EOTF-application logic) and `clutter-color-state-icc.c`
//! (skimmed only, see below). Follows the conventions established in
//! `actor_box.rs`: no `unsafe`, no external crates, `core`/`alloc` only.
//!
//! Skipped, with rationale:
//! - GObject class/property/signal machinery (`G_DECLARE_DERIVABLE_TYPE`,
//!   `ClutterColorStateClass` vtable, `clutter_color_state_get_type`,
//!   `_init`/`_class_init`, `ClutterColorManager`-issued `id` field via
//!   `clutter_color_manager_get_next_id`): this is GObject reference
//!   counting/type-system glue with no equivalent need in this Rust port.
//!   The pure value types (`Colorspace`, `TransferFunction`, `Primaries`,
//!   `Colorimetry`, `Eotf`, `Luminance`) are ported as plain `Copy` structs.
//! - Cogl pipeline/shader-snippet generation: `clutter_color_state_class
//!   ::append_transform_snippet`/`update_uniforms`/`init_color_transform_key`
//!   and all of the `ClutterColorOpSnippet`/`UNIFORM_NAME_*` machinery in
//!   `clutter-color-state-params.c` build GLSL source strings and bind Cogl
//!   pipeline uniforms; this needs a real GPU shader pipeline and is out of
//!   scope here.
//! - `clutter_color_state_add_pipeline_transform`,
//!   `clutter_color_state_update_uniforms`,
//!   `clutter_color_state_do_transform` (the `CoglPipeline`/`float *data`
//!   batch-transform entry points): these dispatch into the Cogl pipeline
//!   and `do_transform_to_XYZ`/`do_transform_from_XYZ` vtable hooks (XYZ
//!   colorimetric conversion via 3x3 matrices derived from primaries +
//!   chromatic adaptation), which live in `clutter-color-state-params.c`
//!   and are themselves bound up with the Cogl pipeline path above. Not
//!   ported.
//! - ICC profile binary parsing (`clutter-color-state-icc.c`, skimmed only):
//!   this wraps `lcms2`/ICC profile structures (`cmsHPROFILE`,
//!   `cmsCreateXYZProfile`, tone-reproduction-curve LUTs sampled from a
//!   parsed `.icc` file) to build a `ClutterColorState` from an arbitrary
//!   ICC profile. This is a large, unrelated binary-format parsing
//!   subsystem with no pure-math equivalent to extract; skipped entirely.
//! - `clutter_color_state_params_new_from_cicp` / `ClutterCicp`: this
//!   pairs `ClutterCicpPrimaries`/`ClutterCicpTransfer` (ITU-T H.273 CICP
//!   code-point enums) with a fallible mapping into `Colorspace`/
//!   `TransferFunction` plus a `GError` for unsupported code points. The
//!   CICP code-point tables themselves are simple integer constants with
//!   no interesting logic beyond a big lookup match; skipped as
//!   out-of-scope GObject/`GError`-flavored API surface, not because of
//!   any unportable math.
//! - `clutter_color_state_to_string`/`clutter_eotf_to_string`/
//!   `clutter_colorimetry_to_string`: debug string formatting via `GString`;
//!   no interesting logic, skipped as it's pure glue around an
//!   allocator-backed string type not used elsewhere in this port.
//! - `clutter_color_state_get_blending`, `clutter_color_state_required_format`,
//!   `clutter_color_state_needs_mapping`: these dispatch into the (skipped)
//!   vtable / depend on `ClutterContext` (global compositor singleton
//!   state) to decide blending color states; no self-contained pure logic
//!   to extract.
//!
//! Ported below:
//! - `Colorspace` (mirrors `ClutterColorspace`): SRGB, BT2020, NTSC, PAL, P3.
//! - `TransferFunction` (mirrors `ClutterTransferFunction`): the named
//!   transfer functions (sRGB piecewise, gamma 2.2, PQ, BT.1886, linear).
//! - `Primaries`/`Colorimetry`/`Eotf`/`Luminance` (mirror the
//!   `ClutterPrimaries`/`ClutterColorimetry`/`ClutterEOTF`/
//!   `ClutterLuminance` structs), including the default primaries table
//!   (`clutter_colorspace_to_primaries`) and default luminance table
//!   (`clutter_eotf_get_default_luminance`).
//! - Equality logic: `chromaticity_equal`, `primaries_equal`,
//!   `colorimetry_equal`, `eotf_equal`, `luminance_value_approx_equal`,
//!   mirroring the static helpers in `clutter-color-state-params.c`
//!   (these stand in for a `Hash`-style "is this the same color state"
//!   comparison; the C source has no separate hash function, it only
//!   defines structural `_equal` helpers, so no hash port was needed
//!   beyond deriving `PartialEq`/`Eq`/`Hash` on the plain-data enums).
//! - EOTF apply/apply-inverse math: `clutter_eotf_apply_gamma22[_inv]`,
//!   `clutter_eotf_apply_srgb_piecewise[_inv]`, `clutter_eotf_apply_pq[_inv]`,
//!   `clutter_eotf_apply_bt1886[_inv]`, `clutter_eotf_apply_gamma`, and the
//!   dispatching `clutter_eotf_apply`/`clutter_eotf_apply_inv`. These are
//!   self-contained scalar math with no GPU/shader dependency.

/// Mirrors `ClutterColorspace`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Colorspace {
    Srgb,
    Bt2020,
    Ntsc,
    Pal,
    P3,
}

/// Mirrors `ClutterTransferFunction`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TransferFunction {
    SrgbPiecewise,
    Gamma22,
    Pq,
    Bt1886,
    Linear,
}

/// Mirrors `ClutterColorimetryType`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ColorimetryType {
    Colorspace,
    Primaries,
}

/// Mirrors `ClutterEOTFType`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EotfType {
    Named,
    Gamma,
}

/// Mirrors `ClutterLuminanceType`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LuminanceType {
    Derived,
    Explicit,
}

/// CIE xy chromaticity coordinates for the red/green/blue/white points of a
/// colorspace's primaries.
///
/// Mirrors `ClutterPrimaries`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Primaries {
    pub r_x: f32,
    pub r_y: f32,
    pub g_x: f32,
    pub g_y: f32,
    pub b_x: f32,
    pub b_y: f32,
    pub w_x: f32,
    pub w_y: f32,
}

/// Mirrors `ClutterColorimetry`: either a named `Colorspace` or explicit
/// `Primaries`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Colorimetry {
    Colorspace(Colorspace),
    Primaries(Primaries),
}

impl Colorimetry {
    /// Mirrors `ClutterColorimetry`'s `.type` discriminant.
    pub fn colorimetry_type(&self) -> ColorimetryType {
        match self {
            Colorimetry::Colorspace(_) => ColorimetryType::Colorspace,
            Colorimetry::Primaries(_) => ColorimetryType::Primaries,
        }
    }
}

/// Mirrors `ClutterEOTF`: either a named transfer function or an explicit
/// gamma exponent.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Eotf {
    Named(TransferFunction),
    Gamma(f32),
}

impl Eotf {
    /// Mirrors `ClutterEOTF`'s `.type` discriminant.
    pub fn eotf_type(&self) -> EotfType {
        match self {
            Eotf::Named(_) => EotfType::Named,
            Eotf::Gamma(_) => EotfType::Gamma,
        }
    }
}

/// Mirrors `ClutterLuminance`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Luminance {
    pub luminance_type: LuminanceType,
    pub min: f32,
    pub max: f32,
    pub reference: f32,
    pub mastering_max: f32,
}

/// Default sRGB / NTSC / PAL / gamma22 luminance values.
///
/// Mirrors the static `sdr_default_luminance` in `clutter-color-state-params.c`.
pub const SDR_DEFAULT_LUMINANCE: Luminance = Luminance {
    luminance_type: LuminanceType::Derived,
    min: 0.2,
    max: 80.0,
    reference: 80.0,
    mastering_max: 80.0,
};

/// Default BT.1886 luminance values.
///
/// Mirrors the static `bt1886_default_luminance`.
pub const BT1886_DEFAULT_LUMINANCE: Luminance = Luminance {
    luminance_type: LuminanceType::Derived,
    min: 0.01,
    max: 100.0,
    reference: 100.0,
    mastering_max: 100.0,
};

/// Default PQ (HDR) luminance values.
///
/// Mirrors the static `pq_default_luminance`.
pub const PQ_DEFAULT_LUMINANCE: Luminance = Luminance {
    luminance_type: LuminanceType::Derived,
    min: 0.005,
    max: 10000.0,
    reference: 203.0,
    mastering_max: 10000.0,
};

/// Returns the default luminance values for a given EOTF.
///
/// Mirrors `clutter_eotf_get_default_luminance`.
pub fn eotf_get_default_luminance(eotf: Eotf) -> Luminance {
    match eotf {
        Eotf::Named(TransferFunction::SrgbPiecewise)
        | Eotf::Named(TransferFunction::Linear)
        | Eotf::Named(TransferFunction::Gamma22) => SDR_DEFAULT_LUMINANCE,
        Eotf::Named(TransferFunction::Bt1886) => BT1886_DEFAULT_LUMINANCE,
        Eotf::Named(TransferFunction::Pq) => PQ_DEFAULT_LUMINANCE,
        Eotf::Gamma(_) => SDR_DEFAULT_LUMINANCE,
    }
}

/// sRGB / BT.709 primaries and D65 white point.
///
/// Mirrors the static `srgb_primaries`.
pub const SRGB_PRIMARIES: Primaries = Primaries {
    r_x: 0.64,
    r_y: 0.33,
    g_x: 0.30,
    g_y: 0.60,
    b_x: 0.15,
    b_y: 0.06,
    w_x: 0.3127,
    w_y: 0.3290,
};

/// NTSC (SMPTE-C) primaries.
///
/// Mirrors the static `ntsc_primaries`.
pub const NTSC_PRIMARIES: Primaries = Primaries {
    r_x: 0.63,
    r_y: 0.34,
    g_x: 0.31,
    g_y: 0.595,
    b_x: 0.155,
    b_y: 0.07,
    w_x: 0.3127,
    w_y: 0.3290,
};

/// BT.2020 (UHDTV) primaries.
///
/// Mirrors the static `bt2020_primaries`.
pub const BT2020_PRIMARIES: Primaries = Primaries {
    r_x: 0.708,
    r_y: 0.292,
    g_x: 0.170,
    g_y: 0.797,
    b_x: 0.131,
    b_y: 0.046,
    w_x: 0.3127,
    w_y: 0.3290,
};

/// PAL primaries.
///
/// Mirrors the static `pal_primaries`.
pub const PAL_PRIMARIES: Primaries = Primaries {
    r_x: 0.64,
    r_y: 0.33,
    g_x: 0.29,
    g_y: 0.60,
    b_x: 0.15,
    b_y: 0.06,
    w_x: 0.3127,
    w_y: 0.3290,
};

/// DCI-P3 primaries.
///
/// Mirrors the static `p3_primaries`.
pub const P3_PRIMARIES: Primaries = Primaries {
    r_x: 0.68,
    r_y: 0.32,
    g_x: 0.265,
    g_y: 0.69,
    b_x: 0.15,
    b_y: 0.06,
    w_x: 0.3127,
    w_y: 0.3290,
};

/// Returns the default primaries for a named colorspace.
///
/// Mirrors `clutter_colorspace_to_primaries`. Note: the upstream C source's
/// exact numeric literals for some of the secondary colorspaces (NTSC, PAL,
/// P3 green/blue chromaticities) were partially illegible in the available
/// source dump for this port; the values above use the well-known standard
/// chromaticities for each colorspace (SMPTE-C for NTSC, EBU for PAL,
/// DCI-P3 with D65 white point) as the best-available substitute, and
/// should be cross-checked against upstream `clutter-color-state-params.c`
/// before being relied on for precise color management.
pub fn colorspace_to_primaries(colorspace: Colorspace) -> Primaries {
    match colorspace {
        Colorspace::Srgb => SRGB_PRIMARIES,
        Colorspace::Ntsc => NTSC_PRIMARIES,
        Colorspace::Bt2020 => BT2020_PRIMARIES,
        Colorspace::Pal => PAL_PRIMARIES,
        Colorspace::P3 => P3_PRIMARIES,
    }
}

/// Returns the effective primaries for a `Colorimetry` value, resolving
/// the `Colorspace` case via `colorspace_to_primaries`.
///
/// Mirrors the static `get_primaries` helper.
pub fn colorimetry_get_primaries(colorimetry: &Colorimetry) -> Primaries {
    match colorimetry {
        Colorimetry::Colorspace(cs) => colorspace_to_primaries(*cs),
        Colorimetry::Primaries(p) => *p,
    }
}

/// Approximate equality with a fixed epsilon, mirroring glib's
/// `G_APPROX_VALUE (a, b, epsilon)` macro: `fabs (a - b) <= epsilon`.
fn approx_value(a: f32, b: f32, epsilon: f32) -> bool {
    f32_abs(a - b) <= epsilon
}

fn f32_abs(v: f32) -> f32 {
    if v < 0.0 {
        -v
    } else {
        v
    }
}

/// Returns whether two chromaticity coordinates are approximately equal.
///
/// Mirrors the static `chromaticity_equal` helper. The `0.0001` epsilon and
/// the upstream `FIXME` about precision are both preserved verbatim from
/// the C source.
// FIXME: the next color managment version will use more precision
pub fn chromaticity_equal(x1: f32, y1: f32, x2: f32, y2: f32) -> bool {
    approx_value(x1, x2, 0.0001) && approx_value(y1, y2, 0.0001)
}

/// Returns whether two sets of primaries are approximately equal.
///
/// Mirrors the static `primaries_equal` helper.
pub fn primaries_equal(a: &Primaries, b: &Primaries) -> bool {
    chromaticity_equal(a.r_x, a.r_y, b.r_x, b.r_y)
        && chromaticity_equal(a.g_x, a.g_y, b.g_x, b.g_y)
        && chromaticity_equal(a.b_x, a.b_y, b.b_x, b.b_y)
        && chromaticity_equal(a.w_x, a.w_y, b.w_x, b.w_y)
}

/// Returns whether two `Colorimetry` values are equivalent: same named
/// colorspace, or (after resolving named colorspaces to primaries)
/// approximately-equal primaries.
///
/// Mirrors the static `colorimetry_equal` helper.
pub fn colorimetry_equal(a: &Colorimetry, b: &Colorimetry) -> bool {
    if let (Colorimetry::Colorspace(a_cs), Colorimetry::Colorspace(b_cs)) = (a, b) {
        return a_cs == b_cs;
    }

    let a_primaries = colorimetry_get_primaries(a);
    let b_primaries = colorimetry_get_primaries(b);
    primaries_equal(&a_primaries, &b_primaries)
}

/// Returns whether two `Eotf` values are equivalent: same named transfer
/// function, or approximately-equal gamma exponents.
///
/// Mirrors the static `eotf_equal` helper.
pub fn eotf_equal(a: &Eotf, b: &Eotf) -> bool {
    match (a, b) {
        (Eotf::Named(a_tf), Eotf::Named(b_tf)) => a_tf == b_tf,
        (Eotf::Gamma(a_exp), Eotf::Gamma(b_exp)) => approx_value(*a_exp, *b_exp, 0.0001),
        _ => false,
    }
}

/// Returns whether two luminance scalar values (e.g. two `min` fields) are
/// approximately equal.
///
/// Mirrors the static `luminance_value_approx_equal` helper (the C source
/// uses the same `0.0001`-class epsilon comparison pattern as
/// `chromaticity_equal`/`eotf_equal` for its float fields).
pub fn luminance_value_approx_equal(a: f32, b: f32) -> bool {
    approx_value(a, b, 0.0001)
}

/// Returns whether two `Luminance` values are equivalent.
///
/// Not a literal 1:1 port of a single named C function (the upstream
/// `ClutterColorStateParams` equality walk inlines per-field luminance
/// comparisons rather than factoring out a `luminance_equal` helper), but
/// composed here from the ported `luminance_value_approx_equal` building
/// block for convenience and to give the `Luminance` type a complete,
/// testable equality story.
pub fn luminance_equal(a: &Luminance, b: &Luminance) -> bool {
    a.luminance_type == b.luminance_type
        && luminance_value_approx_equal(a.min, b.min)
        && luminance_value_approx_equal(a.max, b.max)
        && luminance_value_approx_equal(a.reference, b.reference)
        && luminance_value_approx_equal(a.mastering_max, b.mastering_max)
}

// ---------------------------------------------------------------------
// Hand-rolled float math (no libm in no_std kernel context).
//
// `powf`/`ln`/`exp` are needed for the EOTF curves below (sRGB piecewise,
// gamma curves, and the PQ EOTF, which uses several non-integer exponents
// like `1/0.1593017` and `78.84375`). There is no integer-exponent
// fast path available for these, so a generic `x.powf(y) = exp(y * ln(x))`
// is implemented using hand-rolled `exp`/`ln` series. These are
// double-precision (`f64`) internally for accuracy, then narrowed back to
// `f32`, since the iterative range-reduction relies on `f64` precision to
// stay reasonably close to libm's `powf` over the EOTF input domain
// ([0, 1]-ish for these curves). This is a pure numerical-approximation
// implementation (Taylor/Newton-style), not a libm linkage, and is
// intended only to be "close enough" for this port -- it is not a
// drop-in replacement for a vetted libm `powf` in precision-sensitive
// color-management code paths.
mod mathf {
    /// Natural exponential via a Taylor series around an integer-reduced
    /// argument (`exp(x) = exp(n) * exp(r)` with `r` in `[-0.5, 0.5]`,
    /// `exp(n) = exp(1)^n` via repeated squaring) for fast convergence.
    pub fn exp(x: f64) -> f64 {
        if x.is_nan() {
            return x;
        }
        if x == 0.0 {
            return 1.0;
        }

        const E: f64 = core::f64::consts::E;
        // `f64::round` isn't available without `std`/`libm`; round-half-away
        // -from-zero via truncating add/sub of 0.5 (matches mtk::utils's
        // approach for the same no_std constraint).
        let n = if x >= 0.0 {
            (x + 0.5) as i64 as f64
        } else {
            (x - 0.5) as i64 as f64
        };
        let r = x - n;

        // exp(r) via Taylor series, r in [-0.5, 0.5] converges fast.
        let mut term = 1.0_f64;
        let mut sum = 1.0_f64;
        for k in 1..20 {
            term *= r / (k as f64);
            sum += term;
        }

        // exp(n) = E^n via exponentiation by squaring (n is a small integer
        // for all inputs used by this module's EOTF curves).
        let mut n_i = n as i64;
        let mut neg = false;
        if n_i < 0 {
            neg = true;
            n_i = -n_i;
        }
        let mut base = E;
        let mut result = 1.0_f64;
        while n_i > 0 {
            if n_i & 1 == 1 {
                result *= base;
            }
            base *= base;
            n_i >>= 1;
        }
        if neg {
            result = 1.0 / result;
        }

        result * sum
    }

    /// Natural logarithm via range reduction (`ln(x) = ln(m) + e * ln(2)`
    /// where `x = m * 2^e`, `m` in `[1, 2)`) followed by an
    /// `atanh`-based series for `ln(m)`, which converges quickly for `m`
    /// near 1.
    pub fn ln(x: f64) -> f64 {
        if x <= 0.0 {
            return f64::NAN;
        }
        if x == 1.0 {
            return 0.0;
        }

        let mut m = x;
        let mut e = 0i32;
        while m >= 2.0 {
            m /= 2.0;
            e += 1;
        }
        while m < 1.0 {
            m *= 2.0;
            e -= 1;
        }

        // ln(m) = 2 * atanh((m - 1) / (m + 1)), series in z = (m-1)/(m+1).
        let z = (m - 1.0) / (m + 1.0);
        let z2 = z * z;
        let mut term = z;
        let mut sum = z;
        for k in 1..20 {
            term *= z2;
            sum += term / ((2 * k + 1) as f64);
        }

        const LN2: f64 = core::f64::consts::LN_2;
        2.0 * sum + (e as f64) * LN2
    }

    /// `x.powf(y)` via `exp(y * ln(x))`, with sign handling for negative
    /// `x` raised to integer-ish exponents (only used by the gamma curves
    /// below, which always call this with a non-negative `x` and apply
    /// the sign themselves -- see `eotf_apply_gamma22` etc).
    pub fn powf(x: f32, y: f32) -> f32 {
        if x == 0.0 {
            return if y == 0.0 { 1.0 } else { 0.0 };
        }
        let x = x as f64;
        let y = y as f64;
        exp(y * ln(x)) as f32
    }
}

fn powf(x: f32, y: f32) -> f32 {
    mathf::powf(x, y)
}

fn f32_max(a: f32, b: f32) -> f32 {
    if a > b {
        a
    } else {
        b
    }
}

fn clamp(v: f32, lo: f32, hi: f32) -> f32 {
    if v < lo {
        lo
    } else if v > hi {
        hi
    } else {
        v
    }
}

/// Mirrors `clutter_eotf_apply_gamma22`.
pub fn eotf_apply_gamma22(input: f32) -> f32 {
    if input < 0.0 {
        -powf(-input, 2.2)
    } else {
        powf(input, 2.2)
    }
}

/// Mirrors `clutter_eotf_apply_gamma22_inv`.
pub fn eotf_apply_gamma22_inv(input: f32) -> f32 {
    if input < 0.0 {
        -powf(-input, 1.0 / 2.2)
    } else {
        powf(input, 1.0 / 2.2)
    }
}

/// Mirrors `clutter_eotf_apply_srgb_piecewise`.
pub fn eotf_apply_srgb_piecewise(input: f32) -> f32 {
    if input <= 0.04045 {
        input / 12.92
    } else {
        powf((input + 0.055) / 1.055, 12.0 / 5.0)
    }
}

/// Mirrors `clutter_eotf_apply_srgb_piecewise_inv`.
pub fn eotf_apply_srgb_piecewise_inv(input: f32) -> f32 {
    if input <= 0.0031308 {
        input * 12.92
    } else {
        powf(input, 5.0 / 12.0) * 1.055 - 0.055
    }
}

/// Mirrors `clutter_eotf_apply_pq` (the SMPTE ST 2084 perceptual
/// quantizer EOTF).
pub fn eotf_apply_pq(input: f32) -> f32 {
    let c1 = 0.8359375;
    let c2 = 18.8515625;
    let c3 = 18.6875;
    let oo_m1 = 1.0 / 0.1593017;
    let oo_m2 = 1.0 / 78.84375;
    let input = clamp(input, 0.0, 1.0);
    let num = f32_max(powf(input, oo_m2) - c1, 0.0);
    let den = c2 - c3 * powf(input, oo_m2);
    powf(num / den, oo_m1)
}

/// Mirrors `clutter_eotf_apply_pq_inv`.
pub fn eotf_apply_pq_inv(input: f32) -> f32 {
    let c1 = 0.8359375;
    let c2 = 18.8515625;
    let c3 = 18.6875;
    let m1 = 0.1593017;
    let m2 = 78.84375;
    let input = clamp(input, 0.0, 1.0);
    let in_pow_m1 = powf(input, m1);
    let num = c1 + c2 * in_pow_m1;
    let den = 1.0 + c3 * in_pow_m1;
    powf(num / den, m2)
}

/// Mirrors `clutter_eotf_apply_bt1886`. Assumes an unadjusted display with
/// `L_B = 0`, `L_W = 1`, as noted in the C source.
pub fn eotf_apply_bt1886(input: f32) -> f32 {
    powf(input, 2.4)
}

/// Mirrors `clutter_eotf_apply_bt1886_inv`.
pub fn eotf_apply_bt1886_inv(input: f32) -> f32 {
    powf(input, 1.0 / 2.4)
}

/// Mirrors `clutter_eotf_apply_gamma`. Avoids returning `NaN` for an input
/// of exactly (approximately) zero, matching the C source's
/// `G_APPROX_VALUE (input, 0.0f, FLT_EPSILON)` guard.
pub fn eotf_apply_gamma(input: f32, gamma_exp: f32) -> f32 {
    if approx_value(input, 0.0, f32::EPSILON) {
        0.0
    } else {
        powf(input, gamma_exp)
    }
}

/// Applies the EOTF (electro-optical transfer function), converting an
/// encoded (gamma-compressed) signal value to a linear light value.
///
/// Mirrors `clutter_eotf_apply`. The C source's `g_warning` fallback for
/// unhandled cases can't be reached given `Eotf`'s closed representation
/// (every `Named`/`Gamma` case is handled), so it is omitted.
pub fn eotf_apply(eotf: Eotf, input: f32) -> f32 {
    match eotf {
        Eotf::Named(TransferFunction::Gamma22) => eotf_apply_gamma22(input),
        Eotf::Named(TransferFunction::SrgbPiecewise) => eotf_apply_srgb_piecewise(input),
        Eotf::Named(TransferFunction::Pq) => eotf_apply_pq(input),
        Eotf::Named(TransferFunction::Bt1886) => eotf_apply_bt1886(input),
        Eotf::Named(TransferFunction::Linear) => input,
        Eotf::Gamma(gamma_exp) => eotf_apply_gamma(input, gamma_exp),
    }
}

/// Applies the inverse EOTF (OETF), converting a linear light value to an
/// encoded (gamma-compressed) signal value.
///
/// Mirrors `clutter_eotf_apply_inv`.
pub fn eotf_apply_inv(eotf: Eotf, input: f32) -> f32 {
    match eotf {
        Eotf::Named(TransferFunction::Gamma22) => eotf_apply_gamma22_inv(input),
        Eotf::Named(TransferFunction::SrgbPiecewise) => eotf_apply_srgb_piecewise_inv(input),
        Eotf::Named(TransferFunction::Pq) => eotf_apply_pq_inv(input),
        Eotf::Named(TransferFunction::Bt1886) => eotf_apply_bt1886_inv(input),
        Eotf::Named(TransferFunction::Linear) => input,
        Eotf::Gamma(gamma_exp) => eotf_apply_gamma(input, 1.0 / gamma_exp),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn approx(a: f32, b: f32, eps: f32) -> bool {
        (a - b).abs() <= eps
    }

    #[test]
    fn test_colorspace_variants_distinct() {
        assert_ne!(Colorspace::Srgb, Colorspace::Bt2020);
        assert_eq!(Colorspace::Srgb, Colorspace::Srgb);
    }

    #[test]
    fn test_transfer_function_variants_distinct() {
        assert_ne!(TransferFunction::Pq, TransferFunction::Linear);
        assert_eq!(TransferFunction::Gamma22, TransferFunction::Gamma22);
    }

    #[test]
    fn test_colorimetry_type_discriminant() {
        let by_space = Colorimetry::Colorspace(Colorspace::Srgb);
        let by_primaries = Colorimetry::Primaries(SRGB_PRIMARIES);
        assert_eq!(by_space.colorimetry_type(), ColorimetryType::Colorspace);
        assert_eq!(by_primaries.colorimetry_type(), ColorimetryType::Primaries);
    }

    #[test]
    fn test_eotf_type_discriminant() {
        let named = Eotf::Named(TransferFunction::Linear);
        let gamma = Eotf::Gamma(2.4);
        assert_eq!(named.eotf_type(), EotfType::Named);
        assert_eq!(gamma.eotf_type(), EotfType::Gamma);
    }

    #[test]
    fn test_default_luminance_by_eotf() {
        assert_eq!(
            eotf_get_default_luminance(Eotf::Named(TransferFunction::SrgbPiecewise)),
            SDR_DEFAULT_LUMINANCE
        );
        assert_eq!(
            eotf_get_default_luminance(Eotf::Named(TransferFunction::Linear)),
            SDR_DEFAULT_LUMINANCE
        );
        assert_eq!(
            eotf_get_default_luminance(Eotf::Named(TransferFunction::Gamma22)),
            SDR_DEFAULT_LUMINANCE
        );
        assert_eq!(
            eotf_get_default_luminance(Eotf::Named(TransferFunction::Bt1886)),
            BT1886_DEFAULT_LUMINANCE
        );
        assert_eq!(
            eotf_get_default_luminance(Eotf::Named(TransferFunction::Pq)),
            PQ_DEFAULT_LUMINANCE
        );
        assert_eq!(
            eotf_get_default_luminance(Eotf::Gamma(2.2)),
            SDR_DEFAULT_LUMINANCE
        );
    }

    #[test]
    fn test_colorspace_to_primaries() {
        assert_eq!(colorspace_to_primaries(Colorspace::Srgb), SRGB_PRIMARIES);
        assert_eq!(
            colorspace_to_primaries(Colorspace::Bt2020),
            BT2020_PRIMARIES
        );
    }

    #[test]
    fn test_colorimetry_get_primaries_resolves_colorspace() {
        let c = Colorimetry::Colorspace(Colorspace::Srgb);
        assert_eq!(colorimetry_get_primaries(&c), SRGB_PRIMARIES);

        let custom = Primaries {
            r_x: 0.1,
            r_y: 0.2,
            g_x: 0.3,
            g_y: 0.4,
            b_x: 0.5,
            b_y: 0.6,
            w_x: 0.7,
            w_y: 0.8,
        };
        let c2 = Colorimetry::Primaries(custom);
        assert_eq!(colorimetry_get_primaries(&c2), custom);
    }

    #[test]
    fn test_chromaticity_equal() {
        assert!(chromaticity_equal(0.64, 0.33, 0.64001, 0.33001));
        assert!(!chromaticity_equal(0.64, 0.33, 0.70, 0.33));
    }

    #[test]
    fn test_primaries_equal() {
        assert!(primaries_equal(&SRGB_PRIMARIES, &SRGB_PRIMARIES));
        assert!(!primaries_equal(&SRGB_PRIMARIES, &BT2020_PRIMARIES));
    }

    #[test]
    fn test_colorimetry_equal_same_colorspace_fast_path() {
        let a = Colorimetry::Colorspace(Colorspace::Srgb);
        let b = Colorimetry::Colorspace(Colorspace::Srgb);
        let c = Colorimetry::Colorspace(Colorspace::Bt2020);
        assert!(colorimetry_equal(&a, &b));
        assert!(!colorimetry_equal(&a, &c));
    }

    #[test]
    fn test_colorimetry_equal_mixed_representation() {
        // A Colorspace(Srgb) and an explicit Primaries(SRGB_PRIMARIES)
        // should be considered equal once resolved to primaries.
        let a = Colorimetry::Colorspace(Colorspace::Srgb);
        let b = Colorimetry::Primaries(SRGB_PRIMARIES);
        assert!(colorimetry_equal(&a, &b));
    }

    #[test]
    fn test_eotf_equal() {
        let a = Eotf::Named(TransferFunction::Pq);
        let b = Eotf::Named(TransferFunction::Pq);
        let c = Eotf::Named(TransferFunction::Linear);
        assert!(eotf_equal(&a, &b));
        assert!(!eotf_equal(&a, &c));

        let g1 = Eotf::Gamma(2.2);
        let g2 = Eotf::Gamma(2.20001);
        let g3 = Eotf::Gamma(2.4);
        assert!(eotf_equal(&g1, &g2));
        assert!(!eotf_equal(&g1, &g3));

        // Different variant kinds are never equal.
        assert!(!eotf_equal(&a, &g1));
    }

    #[test]
    fn test_luminance_equal() {
        assert!(luminance_equal(
            &SDR_DEFAULT_LUMINANCE,
            &SDR_DEFAULT_LUMINANCE
        ));
        assert!(!luminance_equal(
            &SDR_DEFAULT_LUMINANCE,
            &PQ_DEFAULT_LUMINANCE
        ));

        let mut tweaked = SDR_DEFAULT_LUMINANCE;
        tweaked.min += 0.00001;
        assert!(luminance_equal(&SDR_DEFAULT_LUMINANCE, &tweaked));
    }

    #[test]
    fn test_mathf_exp_ln_roundtrip() {
        for &x in &[0.001_f64, 0.5, 1.0, 2.0, 10.0, 100.0] {
            let y = mathf::exp(mathf::ln(x));
            assert!((y - x).abs() / x < 1e-6, "exp(ln({x})) = {y}");
        }
    }

    #[test]
    fn test_powf_matches_known_values() {
        assert!(approx(powf(2.0, 10.0), 1024.0, 0.5));
        assert!(approx(powf(4.0, 0.5), 2.0, 0.001));
        assert!(approx(powf(1.0, 123.0), 1.0, 0.0001));
        assert!(approx(powf(0.0, 5.0), 0.0, 0.0001));
        assert!(approx(powf(0.0, 0.0), 1.0, 0.0001));
    }

    #[test]
    fn test_eotf_apply_gamma22_roundtrip() {
        let input = 0.5_f32;
        let encoded = eotf_apply_gamma22(input);
        let decoded = eotf_apply_gamma22_inv(encoded);
        assert!(approx(decoded, input, 0.001), "got {decoded}");
    }

    #[test]
    fn test_eotf_apply_gamma22_negative_input() {
        // Sign should be preserved per the C source's branch on input < 0.
        let pos = eotf_apply_gamma22(0.5);
        let neg = eotf_apply_gamma22(-0.5);
        assert!(approx(neg, -pos, 0.0001));
    }

    #[test]
    fn test_eotf_apply_srgb_piecewise_low_segment() {
        // Below the 0.04045 threshold, the curve is purely linear (/12.92).
        let input = 0.02_f32;
        let expected = input / 12.92;
        assert!(approx(eotf_apply_srgb_piecewise(input), expected, 0.0001));
    }

    #[test]
    fn test_eotf_apply_srgb_piecewise_roundtrip() {
        for &input in &[0.0_f32, 0.02, 0.04045, 0.2, 0.5, 1.0] {
            let encoded_to_linear = eotf_apply_srgb_piecewise(input);
            let back = eotf_apply_srgb_piecewise_inv(encoded_to_linear);
            assert!(
                approx(back, input, 0.01),
                "input={input} linear={encoded_to_linear} back={back}"
            );
        }
    }

    #[test]
    fn test_eotf_apply_pq_bounds() {
        // PQ EOTF should map the [0, 1] domain into [0, 1] roughly, with
        // 0 -> 0 and 1 -> 1 (encoded extremes), and stay within range for
        // an out-of-domain input thanks to the internal clamp.
        assert!(approx(eotf_apply_pq(0.0), 0.0, 0.0005));
        assert!(approx(eotf_apply_pq(1.0), 1.0, 0.01));
        let clamped_high = eotf_apply_pq(5.0);
        let at_one = eotf_apply_pq(1.0);
        assert!(approx(clamped_high, at_one, 0.0001));
    }

    #[test]
    fn test_eotf_apply_pq_roundtrip() {
        for &input in &[0.1_f32, 0.3, 0.5, 0.7, 0.9] {
            let linear = eotf_apply_pq(input);
            let back = eotf_apply_pq_inv(linear);
            assert!(approx(back, input, 0.01), "input={input} back={back}");
        }
    }

    #[test]
    fn test_eotf_apply_bt1886_roundtrip() {
        let input = 0.5_f32;
        let linear = eotf_apply_bt1886(input);
        let back = eotf_apply_bt1886_inv(linear);
        assert!(approx(back, input, 0.001));
    }

    #[test]
    fn test_eotf_apply_gamma_zero_avoids_nan() {
        // Matches the C source's G_APPROX_VALUE guard against NaN for
        // input ~= 0 with a fractional exponent.
        let result = eotf_apply_gamma(0.0, 0.5);
        assert_eq!(result, 0.0);
        assert!(!result.is_nan());
    }

    #[test]
    fn test_eotf_apply_dispatch_matches_individual_fns() {
        let input = 0.42_f32;
        assert_eq!(
            eotf_apply(Eotf::Named(TransferFunction::Linear), input),
            input
        );
        assert_eq!(
            eotf_apply(Eotf::Named(TransferFunction::Gamma22), input),
            eotf_apply_gamma22(input)
        );
        assert_eq!(
            eotf_apply(Eotf::Named(TransferFunction::SrgbPiecewise), input),
            eotf_apply_srgb_piecewise(input)
        );
        assert_eq!(
            eotf_apply(Eotf::Named(TransferFunction::Pq), input),
            eotf_apply_pq(input)
        );
        assert_eq!(
            eotf_apply(Eotf::Named(TransferFunction::Bt1886), input),
            eotf_apply_bt1886(input)
        );
        assert_eq!(
            eotf_apply(Eotf::Gamma(2.4), input),
            eotf_apply_gamma(input, 2.4)
        );
    }

    #[test]
    fn test_eotf_apply_inv_dispatch_matches_individual_fns() {
        let input = 0.42_f32;
        assert_eq!(
            eotf_apply_inv(Eotf::Named(TransferFunction::Linear), input),
            input
        );
        assert_eq!(
            eotf_apply_inv(Eotf::Named(TransferFunction::Gamma22), input),
            eotf_apply_gamma22_inv(input)
        );
        assert_eq!(
            eotf_apply_inv(Eotf::Gamma(2.4), input),
            eotf_apply_gamma(input, 1.0 / 2.4)
        );
    }
}
