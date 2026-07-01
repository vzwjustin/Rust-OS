//! Port of GNOME mutter's `clutter/clutter-shader-types.{c,h}` — shader
//! uniform value types (float, int, matrix variants) used by
//! `ClutterShaderEffect`. GObject boxing/reference-counting machinery is
//! dropped; types are plain enums with inline fixed-size arrays, making
//! them `Copy` and suitable for uniform data.

/// Shader float uniform value: 1, 2, 3, or 4 floats.
/// Mirrors `ClutterShaderFloat`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ShaderFloat {
    /// Single float value.
    Float1(f32),
    /// Two float values (e.g., texture coordinates, 2D scale).
    Float2([f32; 2]),
    /// Three float values (e.g., RGB color, 3D scale).
    Float3([f32; 3]),
    /// Four float values (e.g., RGBA color, quaternion).
    Float4([f32; 4]),
}

impl ShaderFloat {
    /// Returns the number of floats in this value.
    pub fn size(&self) -> usize {
        match self {
            ShaderFloat::Float1(_) => 1,
            ShaderFloat::Float2(_) => 2,
            ShaderFloat::Float3(_) => 3,
            ShaderFloat::Float4(_) => 4,
        }
    }

    /// Extracts floats into a slice (borrows the variant's data).
    pub fn as_slice(&self) -> &[f32] {
        match self {
            ShaderFloat::Float1(v) => core::slice::from_ref(v),
            ShaderFloat::Float2(v) => v,
            ShaderFloat::Float3(v) => v,
            ShaderFloat::Float4(v) => v,
        }
    }
}

/// Shader integer uniform value: 1, 2, 3, or 4 integers.
/// Mirrors `ClutterShaderInt`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShaderInt {
    /// Single integer value.
    Int1(i32),
    /// Two integer values (e.g., texture unit pair, 2D index).
    Int2([i32; 2]),
    /// Three integer values (e.g., 3D index).
    Int3([i32; 3]),
    /// Four integer values (e.g., RGBA channel selector).
    Int4([i32; 4]),
}

impl ShaderInt {
    /// Returns the number of integers in this value.
    pub fn size(&self) -> usize {
        match self {
            ShaderInt::Int1(_) => 1,
            ShaderInt::Int2(_) => 2,
            ShaderInt::Int3(_) => 3,
            ShaderInt::Int4(_) => 4,
        }
    }

    /// Extracts integers into a slice (borrows the variant's data).
    pub fn as_slice(&self) -> &[i32] {
        match self {
            ShaderInt::Int1(v) => core::slice::from_ref(v),
            ShaderInt::Int2(v) => v,
            ShaderInt::Int3(v) => v,
            ShaderInt::Int4(v) => v,
        }
    }
}

/// Shader matrix uniform value: 2x2, 3x3, or 4x4 float matrices.
/// Mirrors `ClutterShaderMatrix`. Matrices are stored in column-major order
/// (standard for OpenGL).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ShaderMatrix {
    /// 2x2 matrix (4 floats).
    Matrix2x2([[f32; 2]; 2]),
    /// 3x3 matrix (9 floats).
    Matrix3x3([[f32; 3]; 3]),
    /// 4x4 matrix (16 floats).
    Matrix4x4([[f32; 4]; 4]),
}

impl ShaderMatrix {
    /// Returns the dimension (2, 3, or 4) of this matrix.
    pub fn dimension(&self) -> usize {
        match self {
            ShaderMatrix::Matrix2x2(_) => 2,
            ShaderMatrix::Matrix3x3(_) => 3,
            ShaderMatrix::Matrix4x4(_) => 4,
        }
    }

    /// Returns the total number of floats in this matrix.
    pub fn size(&self) -> usize {
        let d = self.dimension();
        d * d
    }

    /// Flattens the matrix into a slice (borrows the variant's data).
    pub fn as_slice(&self) -> &[f32] {
        match self {
            ShaderMatrix::Matrix2x2(m) => {
                // SAFETY: `m` is a `&[f32; 4]` reference from the enum variant;
                // the pointer is valid and the length is 4.
                unsafe { core::slice::from_raw_parts(m as *const _ as *const f32, 4) }
            }
            ShaderMatrix::Matrix3x3(m) => {
                // SAFETY: `m` is a `&[f32; 9]` reference from the enum variant;
                // the pointer is valid and the length is 9.
                unsafe { core::slice::from_raw_parts(m as *const _ as *const f32, 9) }
            }
            ShaderMatrix::Matrix4x4(m) => {
                // SAFETY: `m` is a `&[f32; 16]` reference from the enum variant;
                // the pointer is valid and the length is 16.
                unsafe { core::slice::from_raw_parts(m as *const _ as *const f32, 16) }
            }
        }
    }
}

/// Any shader uniform value: float, integer, or matrix.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ShaderValue {
    /// Float uniform.
    Float(ShaderFloat),
    /// Integer uniform.
    Int(ShaderInt),
    /// Matrix uniform.
    Matrix(ShaderMatrix),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shader_float_size() {
        assert_eq!(ShaderFloat::Float1(1.0).size(), 1);
        assert_eq!(ShaderFloat::Float2([1.0, 2.0]).size(), 2);
        assert_eq!(ShaderFloat::Float3([1.0, 2.0, 3.0]).size(), 3);
        assert_eq!(ShaderFloat::Float4([1.0, 2.0, 3.0, 4.0]).size(), 4);
    }

    #[test]
    fn shader_float_as_slice() {
        assert_eq!(ShaderFloat::Float1(1.0).as_slice(), &[1.0]);
        assert_eq!(ShaderFloat::Float2([1.0, 2.0]).as_slice(), &[1.0, 2.0]);
        assert_eq!(
            ShaderFloat::Float3([1.0, 2.0, 3.0]).as_slice(),
            &[1.0, 2.0, 3.0]
        );
        assert_eq!(
            ShaderFloat::Float4([1.0, 2.0, 3.0, 4.0]).as_slice(),
            &[1.0, 2.0, 3.0, 4.0]
        );
    }

    #[test]
    fn shader_int_size() {
        assert_eq!(ShaderInt::Int1(1).size(), 1);
        assert_eq!(ShaderInt::Int2([1, 2]).size(), 2);
        assert_eq!(ShaderInt::Int3([1, 2, 3]).size(), 3);
        assert_eq!(ShaderInt::Int4([1, 2, 3, 4]).size(), 4);
    }

    #[test]
    fn shader_int_as_slice() {
        assert_eq!(ShaderInt::Int1(1).as_slice(), &[1]);
        assert_eq!(ShaderInt::Int2([1, 2]).as_slice(), &[1, 2]);
        assert_eq!(ShaderInt::Int3([1, 2, 3]).as_slice(), &[1, 2, 3]);
        assert_eq!(ShaderInt::Int4([1, 2, 3, 4]).as_slice(), &[1, 2, 3, 4]);
    }

    #[test]
    fn shader_matrix_dimension() {
        assert_eq!(ShaderMatrix::Matrix2x2([[0.0; 2]; 2]).dimension(), 2);
        assert_eq!(ShaderMatrix::Matrix3x3([[0.0; 3]; 3]).dimension(), 3);
        assert_eq!(ShaderMatrix::Matrix4x4([[0.0; 4]; 4]).dimension(), 4);
    }

    #[test]
    fn shader_matrix_size() {
        assert_eq!(ShaderMatrix::Matrix2x2([[0.0; 2]; 2]).size(), 4);
        assert_eq!(ShaderMatrix::Matrix3x3([[0.0; 3]; 3]).size(), 9);
        assert_eq!(ShaderMatrix::Matrix4x4([[0.0; 4]; 4]).size(), 16);
    }

    #[test]
    fn shader_matrix_as_slice() {
        let m22 = ShaderMatrix::Matrix2x2([[1.0, 2.0], [3.0, 4.0]]);
        assert_eq!(m22.as_slice().len(), 4);

        let m33 = ShaderMatrix::Matrix3x3([[1.0, 2.0, 3.0], [4.0, 5.0, 6.0], [7.0, 8.0, 9.0]]);
        assert_eq!(m33.as_slice().len(), 9);

        let m44 = ShaderMatrix::Matrix4x4([
            [1.0, 2.0, 3.0, 4.0],
            [5.0, 6.0, 7.0, 8.0],
            [9.0, 10.0, 11.0, 12.0],
            [13.0, 14.0, 15.0, 16.0],
        ]);
        assert_eq!(m44.as_slice().len(), 16);
    }
}
