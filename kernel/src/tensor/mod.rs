//! # AI-Native Tensor Runtime
//!
//! First-class kernel support for tensor operations and AI inference.
//!
//! ## Design Philosophy
//!
//! Traditional OSes treat GPUs as graphics devices with compute as an afterthought.
//! Nyx treats tensor compute as a first-class citizen:
//!
//! - Tensor buffers are kernel objects with capability-based access control
//! - Automatic migration between CPU/GPU/NPU based on access patterns
//! - Zero-copy sharing between processes via capability grants
//! - Inference contexts managed by kernel for fair scheduling
//!
//! ## Hardware Support
//!
//! - NVIDIA GPUs via CUDA/PTX
//! - AMD GPUs via ROCm/HIP
//! - Intel GPUs via oneAPI
//! - Apple Silicon via Metal
//! - NPUs (Qualcomm Hexagon, Intel NPU, Apple Neural Engine)
//! - CPU fallback via AVX-512/NEON

mod buffer;
mod device;
mod inference;
mod migration;
mod queue;

pub use buffer::{TensorBuffer, TensorShape, DType};
pub use device::{ComputeDevice, DeviceCapabilities, AcceleratorType};
pub use inference::{InferenceContext, InferenceConfig, InferenceRequest};
pub use queue::{ComputeQueue, ComputeCommand};

use crate::cap::{Capability, CapError, ObjectId, ObjectType, Rights};
use spin::RwLock;
use alloc::collections::BTreeMap;
use alloc::vec::Vec;

/// Global tensor buffer registry
static TENSORS: RwLock<BTreeMap<ObjectId, TensorBuffer>> = RwLock::new(BTreeMap::new());

/// Global inference context registry
static CONTEXTS: RwLock<BTreeMap<ObjectId, InferenceContext>> = RwLock::new(BTreeMap::new());

/// Global compute device registry
static DEVICES: RwLock<Vec<ComputeDevice>> = RwLock::new(Vec::new());

/// Check if any AI accelerator is available
pub fn has_accelerator() -> bool {
    // Will be populated during device enumeration
    !DEVICES.read().is_empty()
}

/// Initialize the tensor runtime
pub fn init() {
    log::info!("Initializing tensor runtime");

    // Enumerate compute devices
    enumerate_devices();

    let devices = DEVICES.read();
    log::info!("Found {} compute device(s)", devices.len());

    for device in devices.iter() {
        log::info!(
            "  - {:?}: {} ({} compute units, {} MB memory)",
            device.device_type,
            device.name,
            device.compute_units,
            device.memory_bytes / 1024 / 1024
        );
    }
}

/// Enumerate available compute devices
fn enumerate_devices() {
    let mut devices = DEVICES.write();

    // Always add CPU as fallback
    devices.push(ComputeDevice {
        id: 0,
        device_type: AcceleratorType::Cpu,
        name: alloc::string::String::from("CPU"),
        compute_units: num_cpus(),
        memory_bytes: available_system_memory(),
        capabilities: DeviceCapabilities::CPU_BASELINE,
    });

    // Probe for GPUs
    #[cfg(feature = "cuda")]
    enumerate_cuda_devices(&mut devices);

    #[cfg(feature = "metal")]
    enumerate_metal_devices(&mut devices);

    // Probe for NPUs
    #[cfg(feature = "npu")]
    enumerate_npu_devices(&mut devices);
}

fn num_cpus() -> u32 {
    // TODO: Get from ACPI/device tree
    1
}

fn available_system_memory() -> u64 {
    // TODO: Get from memory manager
    1024 * 1024 * 1024 // 1 GB placeholder
}

#[cfg(feature = "cuda")]
fn enumerate_cuda_devices(_devices: &mut Vec<ComputeDevice>) {
    // TODO: CUDA device enumeration
}

#[cfg(feature = "metal")]
fn enumerate_metal_devices(_devices: &mut Vec<ComputeDevice>) {
    // TODO: Metal device enumeration
}

#[cfg(feature = "npu")]
fn enumerate_npu_devices(_devices: &mut Vec<ComputeDevice>) {
    // TODO: NPU device enumeration
}

/// Allocate a tensor buffer
pub fn tensor_alloc(
    shape: &TensorShape,
    dtype: DType,
    device_id: u32,
) -> Result<Capability, TensorError> {
    let devices = DEVICES.read();
    let device = devices
        .iter()
        .find(|d| d.id == device_id)
        .ok_or(TensorError::DeviceNotFound)?;

    // Calculate buffer size
    let size = shape.total_elements() * dtype.size_bytes();

    // Check device memory
    // TODO: Track per-device memory usage

    let buffer = TensorBuffer {
        id: ObjectId::new(ObjectType::TensorBuffer),
        shape: shape.clone(),
        dtype,
        device_id,
        size_bytes: size,
        // Memory allocation happens in device-specific code
        device_ptr: 0,
        host_ptr: None,
        flags: buffer::TensorFlags::empty(),
    };

    let object_id = buffer.id;
    TENSORS.write().insert(object_id, buffer);

    let cap = unsafe {
        Capability::new_unchecked(
            object_id,
            Rights::READ | Rights::WRITE | Rights::TENSOR_ALLOC |
            Rights::TENSOR_FREE | Rights::TENSOR_MIGRATE | Rights::GRANT,
        )
    };

    Ok(cap)
}

/// Free a tensor buffer
pub fn tensor_free(cap: Capability) -> Result<(), TensorError> {
    cap.require(Rights::TENSOR_FREE)?;

    let mut tensors = TENSORS.write();
    tensors.remove(&cap.object_id).ok_or(TensorError::NotFound)?;

    // TODO: Free device memory

    Ok(())
}

/// Create an inference context
pub fn inference_create(
    model_cap: Capability,
    config: InferenceConfig,
) -> Result<Capability, TensorError> {
    model_cap.require(Rights::MODEL_ACCESS)?;

    let context = InferenceContext::new(model_cap.object_id, config)?;
    let object_id = ObjectId::new(ObjectType::InferenceContext);

    CONTEXTS.write().insert(object_id, context);

    let cap = unsafe {
        Capability::new_unchecked(
            object_id,
            Rights::INFERENCE | Rights::READ | Rights::GRANT,
        )
    };

    Ok(cap)
}

/// Submit an inference request
pub fn inference_submit(
    context_cap: Capability,
    input: Capability,
    params: inference::InferenceParams,
) -> Result<u64, TensorError> {
    context_cap.require(Rights::INFERENCE)?;
    input.require(Rights::READ)?;

    let contexts = CONTEXTS.read();
    let context = contexts
        .get(&context_cap.object_id)
        .ok_or(TensorError::NotFound)?;

    // Submit to inference scheduler
    let request_id = context.submit(input.object_id, params)?;

    Ok(request_id)
}

/// Tensor runtime errors
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TensorError {
    /// Device not found
    DeviceNotFound,
    /// Tensor not found
    NotFound,
    /// Out of memory
    OutOfMemory,
    /// Invalid shape
    InvalidShape,
    /// Device mismatch
    DeviceMismatch,
    /// Inference error
    InferenceError(alloc::string::String),
    /// Capability error
    Capability(CapError),
}

impl From<CapError> for TensorError {
    fn from(err: CapError) -> Self {
        TensorError::Capability(err)
    }
}
