//! Tensor operations

use crate::cap::Capability;
use crate::syscall;

/// Tensor shape
#[derive(Clone, Debug, Default)]
pub struct TensorShape {
    pub dims: [u32; 8],
    pub ndims: u8,
}

impl TensorShape {
    pub fn new(dims: &[u32]) -> Self {
        let mut shape = Self::default();
        shape.ndims = dims.len().min(8) as u8;
        for (i, &d) in dims.iter().take(8).enumerate() {
            shape.dims[i] = d;
        }
        shape
    }

    pub fn vector(size: u32) -> Self {
        Self::new(&[size])
    }

    pub fn matrix(rows: u32, cols: u32) -> Self {
        Self::new(&[rows, cols])
    }
}

/// Data type
#[repr(u8)]
#[derive(Clone, Copy, Debug, Default)]
pub enum DType {
    #[default]
    F32 = 0,
    F16 = 1,
    BF16 = 2,
    I32 = 11,
    U8 = 23,
}

/// Allocate a tensor buffer
pub fn alloc(shape: &TensorShape, dtype: DType, device: u32) -> Result<Capability, i32> {
    let shape_ptr = shape as *const TensorShape as u64;

    let ret = unsafe {
        syscall::syscall3(
            syscall::nr::TENSOR_ALLOC,
            shape_ptr,
            dtype as u64,
            device as u64,
        )
    };

    if ret < 0 {
        Err(ret as i32)
    } else {
        Ok(Capability::from_slot(ret as u32))
    }
}

/// Free a tensor buffer
pub fn free(cap: Capability) -> Result<(), i32> {
    let ret = unsafe {
        syscall::syscall1(syscall::nr::TENSOR_FREE, cap.slot() as u64)
    };

    if ret < 0 {
        Err(ret as i32)
    } else {
        Ok(())
    }
}
