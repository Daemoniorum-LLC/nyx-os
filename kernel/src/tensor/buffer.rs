//! Tensor buffer types and operations

use crate::cap::ObjectId;
use alloc::vec::Vec;
use bitflags::bitflags;

/// Tensor buffer descriptor
#[derive(Clone, Debug)]
pub struct TensorBuffer {
    /// Unique object identifier
    pub id: ObjectId,
    /// Shape (dimensions)
    pub shape: TensorShape,
    /// Data type
    pub dtype: DType,
    /// Device this tensor lives on
    pub device_id: u32,
    /// Size in bytes
    pub size_bytes: u64,
    /// Device memory pointer (GPU/NPU VRAM address)
    pub device_ptr: u64,
    /// Host memory pointer (for unified memory or host-resident)
    pub host_ptr: Option<*mut u8>,
    /// Tensor flags
    pub flags: TensorFlags,
}

// TensorBuffer contains raw pointer but we control access via capabilities
unsafe impl Send for TensorBuffer {}
unsafe impl Sync for TensorBuffer {}

bitflags! {
    /// Tensor buffer flags
    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
    pub struct TensorFlags: u32 {
        /// Tensor is pinned (won't be migrated)
        const PINNED = 1 << 0;
        /// Tensor is in unified memory (CPU+GPU coherent)
        const UNIFIED = 1 << 1;
        /// Tensor is read-only
        const READ_ONLY = 1 << 2;
        /// Tensor is persistent (survives process exit)
        const PERSISTENT = 1 << 3;
        /// Tensor is currently being migrated
        const MIGRATING = 1 << 4;
        /// Tensor has been modified since last sync
        const DIRTY = 1 << 5;
    }
}

/// Tensor shape (up to 8 dimensions)
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TensorShape {
    /// Dimension sizes
    pub dims: [u32; 8],
    /// Number of dimensions
    pub ndims: u8,
}

impl TensorShape {
    /// Create a new tensor shape
    pub fn new(dims: &[u32]) -> Self {
        assert!(dims.len() <= 8, "Maximum 8 dimensions supported");

        let mut shape = Self {
            dims: [1; 8],
            ndims: dims.len() as u8,
        };

        for (i, &dim) in dims.iter().enumerate() {
            shape.dims[i] = dim;
        }

        shape
    }

    /// Create a scalar (0-dimensional tensor)
    pub fn scalar() -> Self {
        Self {
            dims: [1; 8],
            ndims: 0,
        }
    }

    /// Create a 1D tensor
    pub fn vector(size: u32) -> Self {
        Self::new(&[size])
    }

    /// Create a 2D tensor (matrix)
    pub fn matrix(rows: u32, cols: u32) -> Self {
        Self::new(&[rows, cols])
    }

    /// Create a 3D tensor
    pub fn tensor3d(d0: u32, d1: u32, d2: u32) -> Self {
        Self::new(&[d0, d1, d2])
    }

    /// Create a 4D tensor (common for batch + image/sequence)
    pub fn tensor4d(batch: u32, channels: u32, height: u32, width: u32) -> Self {
        Self::new(&[batch, channels, height, width])
    }

    /// Get total number of elements
    pub fn total_elements(&self) -> u64 {
        if self.ndims == 0 {
            return 1;
        }

        self.dims[..self.ndims as usize]
            .iter()
            .map(|&d| d as u64)
            .product()
    }

    /// Get dimension at index
    pub fn dim(&self, idx: usize) -> u32 {
        if idx < self.ndims as usize {
            self.dims[idx]
        } else {
            1
        }
    }

    /// Get number of dimensions
    pub fn rank(&self) -> u8 {
        self.ndims
    }

    /// Check if shapes are compatible for broadcasting
    pub fn broadcast_compatible(&self, other: &TensorShape) -> bool {
        let max_dims = self.ndims.max(other.ndims) as usize;

        for i in 0..max_dims {
            let self_dim = if i < self.ndims as usize {
                self.dims[self.ndims as usize - 1 - i]
            } else {
                1
            };

            let other_dim = if i < other.ndims as usize {
                other.dims[other.ndims as usize - 1 - i]
            } else {
                1
            };

            if self_dim != other_dim && self_dim != 1 && other_dim != 1 {
                return false;
            }
        }

        true
    }
}

/// Data types supported by the tensor runtime
#[repr(u8)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DType {
    /// 32-bit floating point
    #[default]
    F32 = 0,
    /// 16-bit floating point
    F16 = 1,
    /// Brain floating point (16-bit, 8-bit exponent)
    BF16 = 2,
    /// 64-bit floating point
    F64 = 3,

    /// 64-bit signed integer
    I64 = 10,
    /// 32-bit signed integer
    I32 = 11,
    /// 16-bit signed integer
    I16 = 12,
    /// 8-bit signed integer
    I8 = 13,

    /// 64-bit unsigned integer
    U64 = 20,
    /// 32-bit unsigned integer
    U32 = 21,
    /// 16-bit unsigned integer
    U16 = 22,
    /// 8-bit unsigned integer
    U8 = 23,

    /// Boolean (1 byte)
    Bool = 30,

    // === Quantized Types ===

    /// 8-bit quantized (symmetric)
    Q8_0 = 40,
    /// 4-bit quantized (asymmetric)
    Q4_0 = 41,
    /// 4-bit quantized (asymmetric with scales)
    Q4_1 = 42,
    /// 4-bit quantized (K-quant)
    Q4_K = 43,
    /// 5-bit quantized (K-quant)
    Q5_K = 44,
    /// 6-bit quantized (K-quant)
    Q6_K = 45,
    /// 8-bit quantized (K-quant)
    Q8_K = 46,

    // === Special Types ===

    /// 8-bit floating point (E4M3)
    FP8_E4M3 = 50,
    /// 8-bit floating point (E5M2)
    FP8_E5M2 = 51,
}

impl DType {
    /// Get size in bytes for one element
    pub fn size_bytes(&self) -> u64 {
        match self {
            DType::F64 | DType::I64 | DType::U64 => 8,
            DType::F32 | DType::I32 | DType::U32 => 4,
            DType::F16 | DType::BF16 | DType::I16 | DType::U16 => 2,
            DType::I8 | DType::U8 | DType::Bool | DType::Q8_0 | DType::Q8_K => 1,
            DType::FP8_E4M3 | DType::FP8_E5M2 => 1,
            // Quantized types are variable, this is approximate
            DType::Q4_0 | DType::Q4_1 | DType::Q4_K => 1, // ~0.5 bytes per element
            DType::Q5_K => 1,
            DType::Q6_K => 1,
        }
    }

    /// Check if this is a floating point type
    pub fn is_float(&self) -> bool {
        matches!(
            self,
            DType::F64 | DType::F32 | DType::F16 | DType::BF16 |
            DType::FP8_E4M3 | DType::FP8_E5M2
        )
    }

    /// Check if this is a quantized type
    pub fn is_quantized(&self) -> bool {
        matches!(
            self,
            DType::Q8_0 | DType::Q4_0 | DType::Q4_1 |
            DType::Q4_K | DType::Q5_K | DType::Q6_K | DType::Q8_K
        )
    }

    /// Check if this is an integer type
    pub fn is_integer(&self) -> bool {
        matches!(
            self,
            DType::I64 | DType::I32 | DType::I16 | DType::I8 |
            DType::U64 | DType::U32 | DType::U16 | DType::U8
        )
    }
}

/// Tensor slice/view (references part of another tensor)
#[derive(Clone, Debug)]
pub struct TensorView {
    /// Source tensor
    pub source: ObjectId,
    /// Offset in elements from source start
    pub offset: u64,
    /// Shape of this view
    pub shape: TensorShape,
    /// Strides for each dimension
    pub strides: [u64; 8],
}

impl TensorView {
    /// Create a view of the entire tensor
    pub fn full(source: ObjectId, shape: &TensorShape) -> Self {
        let mut strides = [1u64; 8];

        // Calculate default strides (row-major)
        let mut stride = 1u64;
        for i in (0..shape.ndims as usize).rev() {
            strides[i] = stride;
            stride *= shape.dims[i] as u64;
        }

        Self {
            source,
            offset: 0,
            shape: shape.clone(),
            strides,
        }
    }

    /// Check if this view is contiguous in memory
    pub fn is_contiguous(&self) -> bool {
        let mut expected_stride = 1u64;

        for i in (0..self.shape.ndims as usize).rev() {
            if self.strides[i] != expected_stride {
                return false;
            }
            expected_stride *= self.shape.dims[i] as u64;
        }

        true
    }
}
