//! Compute device abstraction

use bitflags::bitflags;
use alloc::string::String;

/// Compute device descriptor
#[derive(Clone, Debug)]
pub struct ComputeDevice {
    /// Device ID (unique within system)
    pub id: u32,
    /// Device type
    pub device_type: AcceleratorType,
    /// Human-readable name
    pub name: String,
    /// Number of compute units (cores/SMs/CUs)
    pub compute_units: u32,
    /// Total device memory in bytes
    pub memory_bytes: u64,
    /// Device capabilities
    pub capabilities: DeviceCapabilities,
}

/// Type of compute accelerator
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AcceleratorType {
    /// CPU (fallback)
    Cpu,
    /// NVIDIA GPU (CUDA)
    NvidiaCuda,
    /// AMD GPU (ROCm/HIP)
    AmdRocm,
    /// Intel GPU (oneAPI)
    IntelOneApi,
    /// Apple GPU (Metal)
    AppleMetal,
    /// Qualcomm Hexagon NPU
    QualcommHexagon,
    /// Intel NPU
    IntelNpu,
    /// Apple Neural Engine
    AppleAne,
    /// Google TPU
    GoogleTpu,
    /// Generic Vulkan compute
    VulkanCompute,
}

bitflags! {
    /// Device capability flags
    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
    pub struct DeviceCapabilities: u64 {
        // === Memory Features ===

        /// Unified memory with CPU
        const UNIFIED_MEMORY = 1 << 0;
        /// Can allocate managed memory
        const MANAGED_MEMORY = 1 << 1;
        /// Supports memory pools
        const MEMORY_POOLS = 1 << 2;
        /// Supports async memory operations
        const ASYNC_MEMORY = 1 << 3;

        // === Compute Features ===

        /// Half-precision (FP16) compute
        const FP16_COMPUTE = 1 << 8;
        /// Brain float (BF16) compute
        const BF16_COMPUTE = 1 << 9;
        /// Double precision (FP64) compute
        const FP64_COMPUTE = 1 << 10;
        /// 8-bit integer compute
        const INT8_COMPUTE = 1 << 11;
        /// 4-bit integer compute
        const INT4_COMPUTE = 1 << 12;
        /// FP8 compute
        const FP8_COMPUTE = 1 << 13;

        // === Tensor Core / Matrix Features ===

        /// Tensor cores / matrix units available
        const TENSOR_CORES = 1 << 16;
        /// Supports sparse matrix operations
        const SPARSE_OPS = 1 << 17;
        /// Hardware matrix multiply-accumulate
        const HARDWARE_MMA = 1 << 18;

        // === Execution Features ===

        /// Supports async compute
        const ASYNC_COMPUTE = 1 << 24;
        /// Supports concurrent kernel execution
        const CONCURRENT_KERNELS = 1 << 25;
        /// Supports preemption
        const PREEMPTION = 1 << 26;
        /// Supports multi-GPU P2P
        const MULTI_GPU_P2P = 1 << 27;
        /// Supports NVLink/Infinity Fabric
        const HIGH_BW_INTERCONNECT = 1 << 28;

        // === Inference Specific ===

        /// Optimized for transformer models
        const TRANSFORMER_OPT = 1 << 32;
        /// Supports flash attention
        const FLASH_ATTENTION = 1 << 33;
        /// Supports paged attention
        const PAGED_ATTENTION = 1 << 34;
        /// Supports speculative decoding
        const SPECULATIVE_DECODE = 1 << 35;

        // === Common Profiles ===

        /// Baseline CPU capabilities
        const CPU_BASELINE = Self::FP64_COMPUTE.bits() |
                            Self::FP16_COMPUTE.bits() |
                            Self::INT8_COMPUTE.bits();

        /// Modern NVIDIA GPU (Ampere+)
        const NVIDIA_MODERN = Self::UNIFIED_MEMORY.bits() |
                             Self::MEMORY_POOLS.bits() |
                             Self::ASYNC_MEMORY.bits() |
                             Self::FP16_COMPUTE.bits() |
                             Self::BF16_COMPUTE.bits() |
                             Self::FP64_COMPUTE.bits() |
                             Self::INT8_COMPUTE.bits() |
                             Self::TENSOR_CORES.bits() |
                             Self::SPARSE_OPS.bits() |
                             Self::ASYNC_COMPUTE.bits() |
                             Self::CONCURRENT_KERNELS.bits() |
                             Self::PREEMPTION.bits() |
                             Self::FLASH_ATTENTION.bits() |
                             Self::PAGED_ATTENTION.bits();

        /// Apple Silicon GPU
        const APPLE_SILICON = Self::UNIFIED_MEMORY.bits() |
                             Self::FP16_COMPUTE.bits() |
                             Self::INT8_COMPUTE.bits() |
                             Self::ASYNC_COMPUTE.bits() |
                             Self::CONCURRENT_KERNELS.bits();
    }
}

impl ComputeDevice {
    /// Check if device supports a specific capability
    pub fn has_capability(&self, cap: DeviceCapabilities) -> bool {
        self.capabilities.contains(cap)
    }

    /// Get estimated peak TFLOPS for this device
    pub fn peak_tflops(&self, dtype: super::DType) -> f64 {
        // Rough estimates based on device type
        match self.device_type {
            AcceleratorType::Cpu => {
                // Assume 4 FLOPS/cycle/core, 3GHz
                (self.compute_units as f64 * 4.0 * 3.0) / 1000.0
            }
            AcceleratorType::NvidiaCuda => {
                // Rough estimate based on compute units
                match dtype {
                    super::DType::F32 => self.compute_units as f64 * 0.1,
                    super::DType::F16 | super::DType::BF16 => self.compute_units as f64 * 0.2,
                    _ => self.compute_units as f64 * 0.05,
                }
            }
            AcceleratorType::AppleMetal => {
                self.compute_units as f64 * 0.05
            }
            _ => 0.0,
        }
    }

    /// Get memory bandwidth estimate (GB/s)
    pub fn memory_bandwidth_gbps(&self) -> f64 {
        match self.device_type {
            AcceleratorType::Cpu => 100.0,  // DDR5 approximate
            AcceleratorType::NvidiaCuda => 900.0,  // HBM3 approximate
            AcceleratorType::AppleMetal => 400.0,  // Unified memory
            _ => 200.0,
        }
    }
}

/// Device memory heap types
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DeviceHeap {
    /// Local device memory (fastest)
    Local,
    /// Host-visible device memory
    HostVisible,
    /// Host-cached device memory
    HostCached,
    /// Unified memory (shared CPU/GPU)
    Unified,
}
