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

/// Per-device memory usage tracking
static DEVICE_MEMORY: RwLock<BTreeMap<u32, DeviceMemoryStats>> = RwLock::new(BTreeMap::new());

/// Device memory usage statistics
#[derive(Clone, Debug, Default)]
pub struct DeviceMemoryStats {
    /// Total memory on device (bytes)
    pub total_bytes: u64,
    /// Currently allocated (bytes)
    pub allocated_bytes: u64,
    /// Peak allocation (bytes)
    pub peak_allocated_bytes: u64,
    /// Number of active allocations
    pub allocation_count: u64,
}

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
///
/// This function probes for all available compute accelerators:
/// - NVIDIA GPUs via PCI (CUDA)
/// - AMD GPUs via PCI (ROCm)
/// - Intel GPUs via PCI (oneAPI)
/// - Apple GPUs via Device Tree (Metal)
/// - Intel NPUs via ACPI
/// - Qualcomm Hexagon via ACPI
/// - Apple Neural Engine via Device Tree
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

    // Probe for GPUs via PCI - always enabled regardless of feature flags
    // The feature flags control runtime support, not detection
    enumerate_nvidia_devices(&mut devices);
    enumerate_amd_devices(&mut devices);
    enumerate_intel_devices(&mut devices);

    // Probe for Apple Silicon GPU (device tree based)
    enumerate_apple_devices(&mut devices);

    // Probe for NPUs
    enumerate_all_npu_devices(&mut devices);

    // Feature-gated probing for additional driver initialization
    #[cfg(feature = "cuda")]
    enumerate_cuda_devices(&mut devices);

    #[cfg(feature = "metal")]
    enumerate_metal_devices(&mut devices);

    #[cfg(feature = "npu")]
    enumerate_npu_devices(&mut devices);
}

fn num_cpus() -> u32 {
    crate::arch::x86_64::smp::cpu_count()
}

fn available_system_memory() -> u64 {
    // Get from memory manager
    crate::mem::get_total_memory().unwrap_or(1024 * 1024 * 1024)
}

// ============================================================================
// PCI Vendor/Device IDs for GPU detection
// ============================================================================

/// NVIDIA vendor ID
const PCI_VENDOR_NVIDIA: u16 = 0x10DE;
/// AMD vendor ID
const PCI_VENDOR_AMD: u16 = 0x1002;
/// Intel vendor ID
const PCI_VENDOR_INTEL: u16 = 0x8086;
/// Apple vendor ID (for virtualized Metal)
const PCI_VENDOR_APPLE: u16 = 0x106B;

/// PCI class code for display controller
const PCI_CLASS_DISPLAY: u8 = 0x03;
/// PCI class code for processing accelerator
const PCI_CLASS_ACCELERATOR: u8 = 0x12;

/// CUDA device enumeration via PCI probing
#[cfg(feature = "cuda")]
fn enumerate_cuda_devices(devices: &mut Vec<ComputeDevice>) {
    enumerate_nvidia_devices(devices);
}

/// Always available - probe PCI for NVIDIA GPUs
fn enumerate_nvidia_devices(devices: &mut Vec<ComputeDevice>) {
    if let Some(pci_devices) = crate::driver::pci::enumerate_devices() {
        for pci_dev in pci_devices {
            if pci_dev.vendor_id == PCI_VENDOR_NVIDIA
                && (pci_dev.class_code == PCI_CLASS_DISPLAY || pci_dev.class_code == PCI_CLASS_ACCELERATOR)
            {
                let device = identify_nvidia_gpu(&pci_dev);
                if let Some(dev) = device {
                    log::info!("Detected NVIDIA GPU: {} (device 0x{:04X})", dev.name, pci_dev.device_id);
                    devices.push(dev);
                }
            }
        }
    }
}

/// Identify NVIDIA GPU capabilities from PCI device ID
fn identify_nvidia_gpu(pci_dev: &crate::driver::pci::PciDevice) -> Option<ComputeDevice> {
    // Device ID ranges for NVIDIA architectures
    // These are simplified - real implementation would have full device tables
    let (arch_name, compute_units, caps) = match pci_dev.device_id {
        // Hopper (H100, etc.) - 0x2300-0x23FF
        0x2300..=0x23FF => (
            "Hopper",
            132u32, // H100 has 132 SMs
            DeviceCapabilities::NVIDIA_MODERN | DeviceCapabilities::FP8_COMPUTE |
            DeviceCapabilities::HIGH_BW_INTERCONNECT | DeviceCapabilities::SPECULATIVE_DECODE
        ),
        // Ada Lovelace (RTX 40xx) - 0x2600-0x27FF
        0x2600..=0x27FF => (
            "Ada Lovelace",
            128u32,
            DeviceCapabilities::NVIDIA_MODERN | DeviceCapabilities::FP8_COMPUTE
        ),
        // Ampere (RTX 30xx, A100) - 0x2200-0x22FF, 0x2000-0x20FF
        0x2000..=0x22FF => (
            "Ampere",
            82u32,
            DeviceCapabilities::NVIDIA_MODERN
        ),
        // Turing (RTX 20xx, T4) - 0x1E00-0x1FFF
        0x1E00..=0x1FFF => (
            "Turing",
            72u32,
            DeviceCapabilities::NVIDIA_MODERN & !DeviceCapabilities::BF16_COMPUTE
        ),
        // Volta (V100) - 0x1D00-0x1DFF
        0x1D00..=0x1DFF => (
            "Volta",
            80u32,
            DeviceCapabilities::NVIDIA_MODERN & !DeviceCapabilities::BF16_COMPUTE &
            !DeviceCapabilities::SPARSE_OPS
        ),
        // Pascal and older - basic CUDA support
        0x1000..=0x1CFF => (
            "Pascal/Maxwell",
            40u32,
            DeviceCapabilities::FP16_COMPUTE | DeviceCapabilities::FP64_COMPUTE |
            DeviceCapabilities::ASYNC_COMPUTE | DeviceCapabilities::CONCURRENT_KERNELS
        ),
        _ => return None,
    };

    // Query VRAM size from BAR1
    let vram_bytes = pci_dev.bar_size(1).unwrap_or(8 * 1024 * 1024 * 1024);

    Some(ComputeDevice {
        id: (devices_count() + 1) as u32,
        device_type: AcceleratorType::NvidiaCuda,
        name: alloc::format!("NVIDIA {} GPU", arch_name),
        compute_units,
        memory_bytes: vram_bytes,
        capabilities: caps,
    })
}

/// AMD GPU enumeration via PCI probing
fn enumerate_amd_devices(devices: &mut Vec<ComputeDevice>) {
    if let Some(pci_devices) = crate::driver::pci::enumerate_devices() {
        for pci_dev in pci_devices {
            if pci_dev.vendor_id == PCI_VENDOR_AMD
                && (pci_dev.class_code == PCI_CLASS_DISPLAY || pci_dev.class_code == PCI_CLASS_ACCELERATOR)
            {
                let device = identify_amd_gpu(&pci_dev);
                if let Some(dev) = device {
                    log::info!("Detected AMD GPU: {} (device 0x{:04X})", dev.name, pci_dev.device_id);
                    devices.push(dev);
                }
            }
        }
    }
}

/// Identify AMD GPU capabilities
fn identify_amd_gpu(pci_dev: &crate::driver::pci::PciDevice) -> Option<ComputeDevice> {
    // AMD device ID ranges (simplified)
    let (arch_name, compute_units, caps) = match pci_dev.device_id {
        // RDNA 3 (RX 7xxx)
        0x7400..=0x74FF | 0x7440..=0x744F => (
            "RDNA 3",
            96u32,
            DeviceCapabilities::FP16_COMPUTE | DeviceCapabilities::INT8_COMPUTE |
            DeviceCapabilities::ASYNC_COMPUTE | DeviceCapabilities::CONCURRENT_KERNELS |
            DeviceCapabilities::TENSOR_CORES
        ),
        // RDNA 2 (RX 6xxx)
        0x73A0..=0x73FF => (
            "RDNA 2",
            80u32,
            DeviceCapabilities::FP16_COMPUTE | DeviceCapabilities::INT8_COMPUTE |
            DeviceCapabilities::ASYNC_COMPUTE | DeviceCapabilities::CONCURRENT_KERNELS
        ),
        // CDNA 3 (MI300)
        0x7408..=0x740F => (
            "CDNA 3",
            228u32,
            DeviceCapabilities::FP16_COMPUTE | DeviceCapabilities::BF16_COMPUTE |
            DeviceCapabilities::FP64_COMPUTE | DeviceCapabilities::INT8_COMPUTE |
            DeviceCapabilities::TENSOR_CORES | DeviceCapabilities::HARDWARE_MMA |
            DeviceCapabilities::HIGH_BW_INTERCONNECT | DeviceCapabilities::FLASH_ATTENTION |
            DeviceCapabilities::PAGED_ATTENTION
        ),
        // CDNA 2 (MI200)
        0x7388..=0x738F => (
            "CDNA 2",
            220u32,
            DeviceCapabilities::FP16_COMPUTE | DeviceCapabilities::BF16_COMPUTE |
            DeviceCapabilities::FP64_COMPUTE | DeviceCapabilities::INT8_COMPUTE |
            DeviceCapabilities::TENSOR_CORES | DeviceCapabilities::HIGH_BW_INTERCONNECT
        ),
        _ => return None,
    };

    let vram_bytes = pci_dev.bar_size(0).unwrap_or(16 * 1024 * 1024 * 1024);

    Some(ComputeDevice {
        id: (devices_count() + 1) as u32,
        device_type: AcceleratorType::AmdRocm,
        name: alloc::format!("AMD {} GPU", arch_name),
        compute_units,
        memory_bytes: vram_bytes,
        capabilities: caps,
    })
}

/// Intel GPU enumeration
fn enumerate_intel_devices(devices: &mut Vec<ComputeDevice>) {
    if let Some(pci_devices) = crate::driver::pci::enumerate_devices() {
        for pci_dev in pci_devices {
            if pci_dev.vendor_id == PCI_VENDOR_INTEL
                && pci_dev.class_code == PCI_CLASS_DISPLAY
            {
                // Check for discrete GPUs (Arc series) vs integrated
                if pci_dev.device_id >= 0x5690 && pci_dev.device_id <= 0x56FF {
                    // Intel Arc discrete GPU
                    let vram = pci_dev.bar_size(0).unwrap_or(16 * 1024 * 1024 * 1024);
                    devices.push(ComputeDevice {
                        id: (devices_count() + 1) as u32,
                        device_type: AcceleratorType::IntelOneApi,
                        name: alloc::string::String::from("Intel Arc GPU"),
                        compute_units: 32,
                        memory_bytes: vram,
                        capabilities: DeviceCapabilities::FP16_COMPUTE |
                            DeviceCapabilities::INT8_COMPUTE |
                            DeviceCapabilities::TENSOR_CORES |
                            DeviceCapabilities::ASYNC_COMPUTE,
                    });
                }
            }
        }
    }
}

/// Metal device enumeration (for Apple Silicon or virtualized environments)
#[cfg(feature = "metal")]
fn enumerate_metal_devices(devices: &mut Vec<ComputeDevice>) {
    enumerate_apple_devices(devices);
}

fn enumerate_apple_devices(devices: &mut Vec<ComputeDevice>) {
    // Check for Apple Silicon via ACPI or device tree
    if let Some(apple_gpu) = detect_apple_silicon_gpu() {
        devices.push(apple_gpu);
    }
}

/// Detect Apple Silicon GPU (M1/M2/M3/M4)
fn detect_apple_silicon_gpu() -> Option<ComputeDevice> {
    // On Apple Silicon, GPU info is in device tree
    // For now, probe via PCI or ACPI
    let dt = crate::driver::devicetree::get_device_tree()?;

    // Look for "apple,gpu" compatible node
    let gpu_node = dt.find_compatible("apple,gpu")?;

    let model = gpu_node.property_string("apple,gpu-model").unwrap_or("Unknown");
    let core_count = gpu_node.property_u32("apple,gpu-core-count").unwrap_or(8);

    // Determine generation from model string
    let (caps, memory_estimate) = if model.contains("M4") || model.contains("m4") {
        (DeviceCapabilities::APPLE_SILICON | DeviceCapabilities::TENSOR_CORES |
         DeviceCapabilities::BF16_COMPUTE | DeviceCapabilities::FLASH_ATTENTION,
         32 * 1024 * 1024 * 1024u64)
    } else if model.contains("M3") || model.contains("m3") {
        (DeviceCapabilities::APPLE_SILICON | DeviceCapabilities::TENSOR_CORES,
         24 * 1024 * 1024 * 1024u64)
    } else if model.contains("M2") || model.contains("m2") {
        (DeviceCapabilities::APPLE_SILICON,
         16 * 1024 * 1024 * 1024u64)
    } else {
        (DeviceCapabilities::APPLE_SILICON,
         8 * 1024 * 1024 * 1024u64)
    };

    Some(ComputeDevice {
        id: (devices_count() + 1) as u32,
        device_type: AcceleratorType::AppleMetal,
        name: alloc::format!("Apple {} GPU", model),
        compute_units: core_count,
        memory_bytes: memory_estimate, // Unified memory
        capabilities: caps,
    })
}

/// NPU device enumeration
#[cfg(feature = "npu")]
fn enumerate_npu_devices(devices: &mut Vec<ComputeDevice>) {
    enumerate_all_npu_devices(devices);
}

fn enumerate_all_npu_devices(devices: &mut Vec<ComputeDevice>) {
    // Intel NPU (Meteor Lake, Lunar Lake)
    if let Some(intel_npu) = detect_intel_npu() {
        devices.push(intel_npu);
    }

    // Qualcomm Hexagon
    if let Some(hexagon) = detect_qualcomm_hexagon() {
        devices.push(hexagon);
    }

    // Apple Neural Engine
    if let Some(ane) = detect_apple_ane() {
        devices.push(ane);
    }
}

/// Detect Intel NPU via ACPI
fn detect_intel_npu() -> Option<ComputeDevice> {
    // Intel NPUs appear as ACPI devices
    let acpi = crate::driver::acpi::get_acpi_tables()?;

    // Look for INT34xx or INTC10xx device IDs
    let npu_dev = acpi.find_device_by_hid(&["INT3472", "INT3400", "INTC1040", "INTC1041"])?;

    let (name, tops) = if npu_dev.hid.starts_with("INTC104") {
        ("Intel Lunar Lake NPU", 48) // ~48 TOPS
    } else if npu_dev.hid.starts_with("INTC103") || npu_dev.hid.starts_with("INT34") {
        ("Intel Meteor Lake NPU", 34) // ~34 TOPS
    } else {
        ("Intel NPU", 10)
    };

    Some(ComputeDevice {
        id: (devices_count() + 1) as u32,
        device_type: AcceleratorType::IntelNpu,
        name: alloc::string::String::from(name),
        compute_units: tops as u32, // TOPS as proxy for compute units
        memory_bytes: 0, // Shared system memory
        capabilities: DeviceCapabilities::INT8_COMPUTE | DeviceCapabilities::INT4_COMPUTE |
            DeviceCapabilities::UNIFIED_MEMORY | DeviceCapabilities::TRANSFORMER_OPT,
    })
}

/// Detect Qualcomm Hexagon DSP/NPU
fn detect_qualcomm_hexagon() -> Option<ComputeDevice> {
    // Qualcomm Hexagon appears via ACPI or as platform device
    let acpi = crate::driver::acpi::get_acpi_tables()?;

    // Look for QCOM Hexagon device
    let hexagon_dev = acpi.find_device_by_hid(&["QCOM0A50", "QCOM24A1", "QCOM0A90"])?;

    Some(ComputeDevice {
        id: (devices_count() + 1) as u32,
        device_type: AcceleratorType::QualcommHexagon,
        name: alloc::string::String::from("Qualcomm Hexagon NPU"),
        compute_units: 45, // TOPS estimate
        memory_bytes: 0,
        capabilities: DeviceCapabilities::INT8_COMPUTE | DeviceCapabilities::INT4_COMPUTE |
            DeviceCapabilities::UNIFIED_MEMORY | DeviceCapabilities::TRANSFORMER_OPT,
    })
}

/// Detect Apple Neural Engine
fn detect_apple_ane() -> Option<ComputeDevice> {
    let dt = crate::driver::devicetree::get_device_tree()?;

    // Look for "apple,ane" compatible node
    let ane_node = dt.find_compatible("apple,ane")?;

    let version = ane_node.property_u32("apple,ane-version").unwrap_or(1);
    let tops = match version {
        4 => 38,  // M4 ANE
        3 => 18,  // M3 ANE
        2 => 15,  // M2 ANE
        _ => 11,  // M1 ANE
    };

    Some(ComputeDevice {
        id: (devices_count() + 1) as u32,
        device_type: AcceleratorType::AppleAne,
        name: alloc::format!("Apple Neural Engine v{}", version),
        compute_units: tops as u32,
        memory_bytes: 0, // Unified memory
        capabilities: DeviceCapabilities::INT8_COMPUTE | DeviceCapabilities::INT4_COMPUTE |
            DeviceCapabilities::UNIFIED_MEMORY | DeviceCapabilities::TRANSFORMER_OPT |
            DeviceCapabilities::FP16_COMPUTE,
    })
}

/// Get current device count (for ID assignment)
fn devices_count() -> usize {
    DEVICES.read().len()
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

    // Calculate buffer size with alignment (64-byte alignment for SIMD)
    let raw_size = shape.total_elements() * dtype.size_bytes();
    let size = (raw_size + 63) & !63; // Round up to 64-byte boundary

    // Check and update device memory tracking
    {
        let mut mem_stats = DEVICE_MEMORY.write();
        let stats = mem_stats.entry(device_id).or_insert_with(|| {
            DeviceMemoryStats {
                total_bytes: device.memory_bytes,
                allocated_bytes: 0,
                peak_allocated_bytes: 0,
                allocation_count: 0,
            }
        });

        // Check if we have enough memory
        if stats.allocated_bytes + size > stats.total_bytes {
            log::warn!(
                "Tensor allocation failed: requested {} bytes, available {} bytes on device {}",
                size,
                stats.total_bytes - stats.allocated_bytes,
                device_id
            );
            return Err(TensorError::OutOfMemory);
        }

        // Reserve the memory
        stats.allocated_bytes += size;
        stats.allocation_count += 1;
        if stats.allocated_bytes > stats.peak_allocated_bytes {
            stats.peak_allocated_bytes = stats.allocated_bytes;
        }
    }

    // Allocate device memory (device-specific)
    let device_ptr = allocate_device_memory(device_id, size)?;

    let buffer = TensorBuffer {
        id: ObjectId::new(ObjectType::TensorBuffer),
        shape: shape.clone(),
        dtype,
        device_id,
        size_bytes: size,
        device_ptr,
        host_ptr: None,
        flags: buffer::TensorFlags::empty(),
    };

    let object_id = buffer.id;

    log::debug!(
        "Allocated tensor {:?}: {} bytes on device {} at 0x{:x}",
        object_id,
        size,
        device_id,
        device_ptr
    );

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

/// Allocate memory on a specific device
fn allocate_device_memory(device_id: u32, size: u64) -> Result<u64, TensorError> {
    let devices = DEVICES.read();
    let device = devices
        .iter()
        .find(|d| d.id == device_id)
        .ok_or(TensorError::DeviceNotFound)?;

    match device.device_type {
        AcceleratorType::Cpu => {
            // CPU: use kernel heap allocation
            // In a real implementation, this would use a dedicated tensor heap
            // For now, return a placeholder address
            Ok(0x1000_0000 + (size & 0xFFFF_0000))
        }
        AcceleratorType::NvidiaCuda => {
            // CUDA: would call cuMemAlloc
            // Placeholder: return fake device pointer
            Ok(0x7F00_0000_0000 + (size & 0xFFFF_0000))
        }
        AcceleratorType::AppleMetal => {
            // Metal: would create MTLBuffer
            // Placeholder: return fake device pointer
            Ok(0x8000_0000_0000 + (size & 0xFFFF_0000))
        }
        _ => {
            // Generic fallback
            Ok(0x9000_0000_0000 + (size & 0xFFFF_0000))
        }
    }
}

/// Free memory on a specific device
fn free_device_memory(device_id: u32, device_ptr: u64, size: u64) {
    let devices = DEVICES.read();
    if let Some(device) = devices.iter().find(|d| d.id == device_id) {
        match device.device_type {
            AcceleratorType::Cpu => {
                // CPU: would free from kernel heap
                log::trace!("Free CPU tensor memory at 0x{:x}", device_ptr);
            }
            AcceleratorType::NvidiaCuda => {
                // CUDA: would call cuMemFree
                log::trace!("Free CUDA tensor memory at 0x{:x}", device_ptr);
            }
            AcceleratorType::AppleMetal => {
                // Metal: would release MTLBuffer
                log::trace!("Free Metal tensor memory at 0x{:x}", device_ptr);
            }
            _ => {
                log::trace!("Free tensor memory at 0x{:x}", device_ptr);
            }
        }
    }
    let _ = (device_ptr, size); // Suppress unused warnings in placeholder impl
}

/// Free a tensor buffer
pub fn tensor_free(cap: Capability) -> Result<(), TensorError> {
    cap.require(Rights::TENSOR_FREE)?;

    let mut tensors = TENSORS.write();
    let buffer = tensors.remove(&cap.object_id).ok_or(TensorError::NotFound)?;

    // Free device memory
    free_device_memory(buffer.device_id, buffer.device_ptr, buffer.size_bytes);

    // Update memory tracking
    {
        let mut mem_stats = DEVICE_MEMORY.write();
        if let Some(stats) = mem_stats.get_mut(&buffer.device_id) {
            stats.allocated_bytes = stats.allocated_bytes.saturating_sub(buffer.size_bytes);
            stats.allocation_count = stats.allocation_count.saturating_sub(1);
        }
    }

    log::debug!(
        "Freed tensor {:?}: {} bytes on device {}",
        cap.object_id,
        buffer.size_bytes,
        buffer.device_id
    );

    Ok(())
}

/// Get memory statistics for a device
pub fn get_device_memory_stats(device_id: u32) -> Option<DeviceMemoryStats> {
    DEVICE_MEMORY.read().get(&device_id).cloned()
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

    let mut contexts = CONTEXTS.write();
    let context = contexts
        .get_mut(&context_cap.object_id)
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
    /// Request queue is full
    QueueFull,
}

impl From<CapError> for TensorError {
    fn from(err: CapError) -> Self {
        TensorError::Capability(err)
    }
}

// ============================================================================
// IPC Helper Functions
// ============================================================================

/// Allocate a buffer (IPC interface)
/// Returns (buffer_id, physical_address)
pub fn allocate_buffer(
    size: u64,
    device_type: u32,
    _alignment: u64,
) -> Result<(u64, u64), TensorError> {
    // Find appropriate device
    let device_id = match device_type {
        0 => 0, // CPU
        1 => find_gpu_device().unwrap_or(0), // GPU (fallback to CPU)
        2 => find_npu_device().unwrap_or(0), // NPU (fallback to CPU)
        _ => 0,
    };

    // Calculate shape from size (treat as 1D buffer)
    let shape = TensorShape::vector(size as u32);

    // Allocate buffer
    let cap = tensor_alloc(&shape, DType::U8, device_id)?;

    // Get the buffer's physical address
    let tensors = TENSORS.read();
    let buffer = tensors.get(&cap.object_id).ok_or(TensorError::NotFound)?;

    Ok((cap.object_id.as_u64(), buffer.device_ptr))
}

/// Submit inference request (IPC interface)
pub fn submit_inference(
    model_id: u64,
    input_buffer: u64,
    _output_buffer: u64,
    _flags: u32,
) -> Result<u64, TensorError> {
    // Look up inference context by model_id
    let mut contexts = CONTEXTS.write();

    // Find context for this model
    let context_id = ObjectId::from_raw(model_id);
    let context = contexts.get_mut(&context_id).ok_or(TensorError::NotFound)?;

    // Create inference params with default balanced sampling
    let params = inference::InferenceParams::balanced();

    // Submit request
    let input_id = ObjectId::from_raw(input_buffer);
    let request_id = context.submit(input_id, params)?;

    Ok(request_id)
}

/// Find first GPU device
fn find_gpu_device() -> Option<u32> {
    let devices = DEVICES.read();
    devices.iter()
        .find(|d| matches!(d.device_type,
            AcceleratorType::NvidiaCuda | AcceleratorType::AmdRocm |
            AcceleratorType::IntelOneApi | AcceleratorType::AppleMetal |
            AcceleratorType::VulkanCompute))
        .map(|d| d.id)
}

/// Find first NPU device
fn find_npu_device() -> Option<u32> {
    let devices = DEVICES.read();
    devices.iter()
        .find(|d| matches!(d.device_type,
            AcceleratorType::QualcommHexagon | AcceleratorType::IntelNpu |
            AcceleratorType::AppleAne | AcceleratorType::GoogleTpu))
        .map(|d| d.id)
}

// ============================================================================
// Tensor Migration Functions
// ============================================================================

pub use migration::MigrationStrategy;

/// Global migration scheduler
static MIGRATION_SCHEDULER: RwLock<migration::MigrationScheduler> =
    RwLock::new(migration::MigrationScheduler::new_const());

/// Get the device ID where a tensor is currently located
pub fn get_tensor_device(tensor_id: ObjectId) -> Option<u32> {
    TENSORS.read().get(&tensor_id).map(|t| t.device_id)
}

/// Schedule an asynchronous tensor migration
///
/// Returns a job ID that can be used to track migration progress.
pub fn schedule_migration(
    tensor_id: ObjectId,
    src_device: u32,
    dst_device: u32,
) -> u64 {
    MIGRATION_SCHEDULER
        .write()
        .schedule(tensor_id, src_device, dst_device)
}

/// Perform synchronous tensor migration
///
/// Blocks until migration is complete.
pub fn migrate_sync(
    tensor_id: ObjectId,
    src_device: u32,
    dst_device: u32,
    strategy: MigrationStrategy,
) -> Result<(), TensorError> {
    // Get the tensor buffer
    let mut tensors = TENSORS.write();
    let tensor = tensors.get_mut(&tensor_id).ok_or(TensorError::NotFound)?;

    // Validate source device
    if tensor.device_id != src_device {
        return Err(TensorError::DeviceMismatch);
    }

    // Get device info
    let devices = DEVICES.read();
    let dst_dev = devices
        .iter()
        .find(|d| d.id == dst_device)
        .ok_or(TensorError::DeviceNotFound)?;

    // Perform migration based on strategy
    match strategy {
        MigrationStrategy::Sync => {
            // Direct copy through CPU
            migrate_through_cpu(tensor, dst_device, dst_dev)?;
        }
        MigrationStrategy::Staged => {
            // Copy to host memory first, then to device
            migrate_through_cpu(tensor, dst_device, dst_dev)?;
        }
        MigrationStrategy::Async => {
            // For sync call, fall back to CPU path
            migrate_through_cpu(tensor, dst_device, dst_dev)?;
        }
        MigrationStrategy::P2P => {
            // Attempt P2P, fall back to CPU if not available
            if !try_p2p_migration(tensor, src_device, dst_device) {
                migrate_through_cpu(tensor, dst_device, dst_dev)?;
            }
        }
    }

    // Update tensor's device ID
    tensor.device_id = dst_device;

    log::debug!(
        "Migrated tensor {:?} from device {} to device {}",
        tensor_id,
        src_device,
        dst_device
    );

    Ok(())
}

/// Migrate tensor through CPU memory (staging)
fn migrate_through_cpu(
    tensor: &mut TensorBuffer,
    dst_device: u32,
    dst_dev: &ComputeDevice,
) -> Result<(), TensorError> {
    // If tensor is already on CPU, just update device pointer
    if tensor.device_id == 0 {
        // Allocate on destination device
        // For now, we keep the same pointer (placeholder)
        // Real implementation would call device-specific allocation
        return Ok(());
    }

    // Allocate host staging buffer if needed
    if tensor.host_ptr.is_none() {
        let host_buffer = alloc::vec![0u8; tensor.size_bytes as usize];
        let ptr = host_buffer.leak().as_mut_ptr() as u64;
        tensor.host_ptr = Some(ptr);
    }

    // Copy from source device to host
    // (In real implementation, this would use DMA or device API)
    copy_device_to_host(tensor)?;

    // Copy from host to destination device
    copy_host_to_device(tensor, dst_device)?;

    Ok(())
}

/// Try peer-to-peer migration between GPUs
fn try_p2p_migration(
    tensor: &mut TensorBuffer,
    src_device: u32,
    dst_device: u32,
) -> bool {
    // Check if P2P is available between these devices
    // For now, always return false (not implemented)
    false
}

/// Copy tensor data from device to host memory
fn copy_device_to_host(tensor: &mut TensorBuffer) -> Result<(), TensorError> {
    // Placeholder - real implementation would use:
    // - cudaMemcpy for NVIDIA
    // - hipMemcpy for AMD
    // - Metal blit for Apple
    Ok(())
}

/// Copy tensor data from host to device memory
fn copy_host_to_device(tensor: &mut TensorBuffer, dst_device: u32) -> Result<(), TensorError> {
    // Placeholder - real implementation would use device-specific API
    Ok(())
}

/// Check migration job status
pub fn migration_status(job_id: u64) -> Option<migration::MigrationStatus> {
    // For now, just return completed (async not fully implemented)
    Some(migration::MigrationStatus::Completed)
}

// ============================================================================
// Memory Mapping Support
// ============================================================================

/// Get the physical frame for a tensor buffer at a given offset
///
/// This is called from the virtual memory fault handler when a
/// tensor-backed VMA needs to be mapped. For CPU tensors, this returns
/// the physical address directly. For GPU/NPU tensors, this may trigger
/// a migration to CPU memory first.
pub fn get_tensor_frame(tensor_id: ObjectId, offset: u64) -> Option<crate::mem::PhysAddr> {
    let tensors = TENSORS.read();
    let tensor = tensors.get(&tensor_id)?;

    // Check if offset is within tensor bounds
    if offset >= tensor.size_bytes {
        log::warn!(
            "Tensor frame access out of bounds: offset {} >= size {}",
            offset,
            tensor.size_bytes
        );
        return None;
    }

    // For CPU tensors, we can compute the physical address directly
    if tensor.device_id == 0 {
        // CPU tensor: device_ptr is the base physical address
        // Calculate the page-aligned address
        let page_offset = offset & !(crate::mem::PAGE_SIZE - 1);
        let phys_addr = tensor.device_ptr + page_offset;
        return Some(crate::mem::PhysAddr::new(phys_addr));
    }

    // For GPU/NPU tensors, we need to access through the host pointer
    // If no host pointer exists, the tensor needs to be migrated first
    if let Some(host_ptr) = tensor.host_ptr {
        let page_offset = offset & !(crate::mem::PAGE_SIZE - 1);
        // Offset the pointer and convert to physical address
        let phys_addr = unsafe { host_ptr.add(page_offset as usize) } as u64;
        return Some(crate::mem::PhysAddr::new(phys_addr));
    }

    // Tensor is on GPU/NPU without host mapping
    // This should trigger a migration, but for now return None
    // to indicate the mapping failed and needs explicit migration
    log::debug!(
        "Tensor {:?} on device {} has no host mapping for offset {}",
        tensor_id,
        tensor.device_id,
        offset
    );

    None
}
