//! Tensor and AI inference interface
//!
//! Nyx provides first-class support for AI/ML workloads with:
//! - Unified tensor memory across CPU, GPU, and NPU
//! - Automatic device placement and migration
//! - Batched inference submission
//!
//! # Example
//! ```no_run
//! // Allocate input/output buffers
//! let input = TensorBuffer::alloc(1024, Device::Gpu, 64)?;
//! let output = TensorBuffer::alloc(256, Device::Gpu, 64)?;
//!
//! // Submit inference
//! let request_id = inference_submit(model_id, input.id(), output.id(), 0)?;
//! ```

use crate::cap::Capability;
use crate::syscall::{self, nr, Error};

/// Device types for tensor allocation
#[repr(u32)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Device {
    /// CPU memory
    #[default]
    Cpu = 0,
    /// GPU memory (CUDA/Metal/Vulkan)
    Gpu = 1,
    /// NPU memory (Apple Neural Engine, etc.)
    Npu = 2,
    /// Unified memory (accessible by all devices)
    Unified = 3,
}

/// Tensor shape (up to 8 dimensions)
#[derive(Clone, Debug, Default)]
pub struct TensorShape {
    /// Dimension sizes
    pub dims: [u32; 8],
    /// Number of dimensions
    pub ndims: u8,
}

impl TensorShape {
    /// Create a new shape from dimensions
    pub fn new(dims: &[u32]) -> Self {
        let mut shape = Self::default();
        shape.ndims = dims.len().min(8) as u8;
        for (i, &d) in dims.iter().take(8).enumerate() {
            shape.dims[i] = d;
        }
        shape
    }

    /// Create a 1D vector shape
    pub fn vector(size: u32) -> Self {
        Self::new(&[size])
    }

    /// Create a 2D matrix shape
    pub fn matrix(rows: u32, cols: u32) -> Self {
        Self::new(&[rows, cols])
    }

    /// Create a 3D tensor shape
    pub fn tensor3d(d0: u32, d1: u32, d2: u32) -> Self {
        Self::new(&[d0, d1, d2])
    }

    /// Create a 4D tensor shape (common for batched images)
    pub fn tensor4d(batch: u32, channels: u32, height: u32, width: u32) -> Self {
        Self::new(&[batch, channels, height, width])
    }

    /// Get total number of elements
    pub fn numel(&self) -> usize {
        self.dims[..self.ndims as usize]
            .iter()
            .map(|&d| d as usize)
            .product()
    }
}

/// Data type for tensor elements
#[repr(u8)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DType {
    /// 32-bit float
    #[default]
    F32 = 0,
    /// 16-bit float (IEEE 754)
    F16 = 1,
    /// 16-bit bfloat
    BF16 = 2,
    /// 64-bit float
    F64 = 3,
    /// 8-bit signed integer
    I8 = 10,
    /// 16-bit signed integer
    I16 = 11,
    /// 32-bit signed integer
    I32 = 12,
    /// 64-bit signed integer
    I64 = 13,
    /// 8-bit unsigned integer
    U8 = 20,
    /// 16-bit unsigned integer
    U16 = 21,
    /// 32-bit unsigned integer
    U32 = 22,
    /// 64-bit unsigned integer
    U64 = 23,
    /// Boolean
    Bool = 30,
}

impl DType {
    /// Get size of this dtype in bytes
    pub fn size_bytes(&self) -> usize {
        match self {
            DType::Bool | DType::I8 | DType::U8 => 1,
            DType::F16 | DType::BF16 | DType::I16 | DType::U16 => 2,
            DType::F32 | DType::I32 | DType::U32 => 4,
            DType::F64 | DType::I64 | DType::U64 => 8,
        }
    }
}

/// Tensor buffer handle
#[derive(Clone, Copy, Debug)]
pub struct TensorBuffer {
    /// Buffer ID (capability)
    id: u64,
    /// Size in bytes
    size: u64,
    /// Device type
    device: Device,
}

impl TensorBuffer {
    /// Allocate a new tensor buffer
    ///
    /// # Arguments
    /// * `size` - Size in bytes
    /// * `device` - Target device
    /// * `alignment` - Alignment in bytes (0 = default, must be power of 2)
    ///
    /// # Example
    /// ```no_run
    /// // Allocate 1MB on GPU with 256-byte alignment
    /// let buffer = TensorBuffer::alloc(1024 * 1024, Device::Gpu, 256)?;
    /// ```
    pub fn alloc(size: u64, device: Device, alignment: u64) -> Result<Self, Error> {
        let result =
            unsafe { syscall::syscall3(nr::TENSOR_ALLOC, size, device as u64, alignment) };

        let id = Error::from_raw(result)?;

        Ok(Self { id, size, device })
    }

    /// Allocate a tensor buffer for a given shape and dtype
    pub fn alloc_for(shape: &TensorShape, dtype: DType, device: Device) -> Result<Self, Error> {
        let size = shape.numel() * dtype.size_bytes();
        Self::alloc(size as u64, device, 0)
    }

    /// Get the buffer ID
    pub fn id(&self) -> u64 {
        self.id
    }

    /// Get the buffer size
    pub fn size(&self) -> u64 {
        self.size
    }

    /// Get the device type
    pub fn device(&self) -> Device {
        self.device
    }

    /// Free this buffer
    ///
    /// After calling this, the buffer handle is invalid.
    pub fn free(self) -> Result<(), Error> {
        let result = unsafe { syscall::syscall1(nr::TENSOR_FREE, self.id) };
        Error::from_raw(result).map(|_| ())
    }

    /// Migrate buffer to a different device
    ///
    /// # Arguments
    /// * `target_device` - Device to migrate to
    ///
    /// # Returns
    /// New buffer handle on target device
    pub fn migrate(&self, target_device: Device) -> Result<TensorBuffer, Error> {
        let result =
            unsafe { syscall::syscall2(nr::TENSOR_MIGRATE, self.id, target_device as u64) };

        let new_id = Error::from_raw(result)?;

        Ok(Self {
            id: new_id,
            size: self.size,
            device: target_device,
        })
    }
}

/// Inference context configuration
#[repr(C)]
#[derive(Clone, Debug, Default)]
pub struct InferenceConfig {
    /// Maximum batch size
    pub max_batch_size: u32,
    /// Timeout in milliseconds
    pub timeout_ms: u32,
    /// Preferred device
    pub device: u32,
    /// Reserved for future use
    pub _reserved: [u32; 5],
}

/// Create an inference context
///
/// # Arguments
/// * `model` - Model capability
/// * `config` - Inference configuration
///
/// # Returns
/// Context capability for submitting inference requests
pub fn inference_create(model: Capability, config: &InferenceConfig) -> Result<Capability, Error> {
    let config_ptr = config as *const InferenceConfig as u64;
    let config_len = core::mem::size_of::<InferenceConfig>() as u64;

    let result =
        unsafe { syscall::syscall3(nr::INFERENCE_CREATE, model.as_raw(), config_ptr, config_len) };

    Error::from_raw(result).map(Capability::from_raw)
}

/// Submit an inference request
///
/// # Arguments
/// * `model_id` - Model ID
/// * `input_buffer` - Input tensor buffer ID
/// * `output_buffer` - Output tensor buffer ID
/// * `flags` - Request flags (see `flags` module)
///
/// # Returns
/// Request ID for tracking completion
///
/// # Example
/// ```no_run
/// let input = TensorBuffer::alloc(1024, Device::Gpu, 0)?;
/// let output = TensorBuffer::alloc(256, Device::Gpu, 0)?;
///
/// // Fill input buffer with data...
///
/// let request_id = inference_submit(model_id, input.id(), output.id(), 0)?;
///
/// // Wait for completion via IPC notification...
/// ```
pub fn inference_submit(
    model_id: u64,
    input_buffer: u64,
    output_buffer: u64,
    flags: u32,
) -> Result<u64, Error> {
    let result = unsafe {
        syscall::syscall4(
            nr::INFERENCE_SUBMIT,
            model_id,
            input_buffer,
            output_buffer,
            flags as u64,
        )
    };

    Error::from_raw(result)
}

/// Inference submission flags
pub mod flags {
    /// Synchronous (wait for completion)
    pub const SYNC: u32 = 1 << 0;
    /// High priority
    pub const HIGH_PRIORITY: u32 = 1 << 1;
    /// Low latency (prefer smaller batches)
    pub const LOW_LATENCY: u32 = 1 << 2;
}

// ============================================================================
// Legacy compatibility
// ============================================================================

/// Allocate a tensor buffer (legacy API)
#[deprecated(note = "Use TensorBuffer::alloc_for instead")]
pub fn alloc(shape: &TensorShape, dtype: DType, _device: u32) -> Result<Capability, Error> {
    let buffer = TensorBuffer::alloc_for(shape, dtype, Device::Cpu)?;
    Ok(Capability::from_raw(buffer.id()))
}

/// Free a tensor buffer (legacy API)
#[deprecated(note = "Use TensorBuffer::free instead")]
pub fn free(cap: Capability) -> Result<(), Error> {
    let result = unsafe { syscall::syscall1(nr::TENSOR_FREE, cap.as_raw()) };
    Error::from_raw(result).map(|_| ())
}
