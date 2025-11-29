//! GPU/NPU compute queue

use crate::cap::ObjectId;
use alloc::vec::Vec;

/// Compute queue for GPU/NPU work
pub struct ComputeQueue {
    /// Queue ID
    pub id: ObjectId,
    /// Target device
    pub device_id: u32,
    /// Pending commands
    commands: Vec<ComputeCommand>,
    /// Maximum queue depth
    max_depth: usize,
}

/// Compute command
#[derive(Clone, Debug)]
pub enum ComputeCommand {
    /// Dispatch compute kernel
    Dispatch {
        kernel: KernelHandle,
        grid: [u32; 3],
        block: [u32; 3],
        args: Vec<ComputeArg>,
    },
    /// Memory copy
    Copy {
        src: ObjectId,
        dst: ObjectId,
        size: u64,
    },
    /// Memory barrier
    Barrier {
        scope: BarrierScope,
    },
    /// Signal completion
    Signal {
        value: u64,
    },
    /// Wait for signal
    Wait {
        value: u64,
    },
}

/// Compute kernel handle
#[derive(Clone, Debug)]
pub struct KernelHandle {
    /// Kernel code (SPIR-V, PTX, etc.)
    pub code: ObjectId,
    /// Entry point name
    pub entry: alloc::string::String,
}

/// Compute argument
#[derive(Clone, Debug)]
pub enum ComputeArg {
    /// Tensor buffer
    Tensor(ObjectId),
    /// Scalar value
    Scalar(ScalarValue),
    /// Constant buffer
    Constant(Vec<u8>),
}

/// Scalar value for compute args
#[derive(Clone, Copy, Debug)]
pub enum ScalarValue {
    U32(u32),
    U64(u64),
    I32(i32),
    I64(i64),
    F32(f32),
    F64(f64),
}

/// Barrier scope
#[derive(Clone, Copy, Debug)]
pub enum BarrierScope {
    /// Device-wide barrier
    Device,
    /// Queue-only barrier
    Queue,
    /// Memory barrier only
    Memory,
}

impl ComputeQueue {
    /// Create a new compute queue
    pub fn new(device_id: u32, max_depth: usize) -> Self {
        Self {
            id: ObjectId::new(crate::cap::ObjectType::ComputeQueue),
            device_id,
            commands: Vec::new(),
            max_depth,
        }
    }

    /// Submit a command
    pub fn submit(&mut self, cmd: ComputeCommand) -> Result<(), super::TensorError> {
        if self.commands.len() >= self.max_depth {
            return Err(super::TensorError::OutOfMemory);
        }

        self.commands.push(cmd);
        Ok(())
    }

    /// Get pending command count
    pub fn pending(&self) -> usize {
        self.commands.len()
    }

    /// Pop next command for execution
    pub fn pop(&mut self) -> Option<ComputeCommand> {
        if self.commands.is_empty() {
            None
        } else {
            Some(self.commands.remove(0))
        }
    }

    /// Clear all pending commands
    pub fn clear(&mut self) {
        self.commands.clear();
    }
}
