//! Inference context and execution

use crate::cap::ObjectId;
use alloc::collections::VecDeque;
use alloc::string::String;
use core::sync::atomic::{AtomicU64, Ordering};

/// Next request ID counter
static NEXT_REQUEST_ID: AtomicU64 = AtomicU64::new(1);

/// Inference context - manages model execution
#[derive(Debug)]
pub struct InferenceContext {
    /// Model being used
    pub model_id: ObjectId,
    /// Configuration
    pub config: InferenceConfig,
    /// Pending requests
    pub pending: VecDeque<InferenceRequest>,
    /// Statistics
    pub stats: InferenceStats,
}

/// Inference configuration
#[derive(Clone, Debug, Default)]
pub struct InferenceConfig {
    /// Target device
    pub device_id: u32,
    /// Maximum context length
    pub max_context_length: u32,
    /// Maximum batch size
    pub max_batch_size: u32,
    /// KV cache size in bytes
    pub kv_cache_size: u64,
    /// Enable continuous batching
    pub continuous_batching: bool,
    /// Enable speculative decoding
    pub speculative_decoding: Option<SpeculativeConfig>,
}

/// Speculative decoding configuration
#[derive(Clone, Debug)]
pub struct SpeculativeConfig {
    /// Draft model ID
    pub draft_model: ObjectId,
    /// Number of draft tokens per step
    pub draft_tokens: u32,
}

/// Inference request
#[derive(Debug)]
pub struct InferenceRequest {
    /// Request ID
    pub id: u64,
    /// Input tensor ID
    pub input: ObjectId,
    /// Sampling parameters
    pub params: InferenceParams,
    /// Current state
    pub state: RequestState,
}

/// Sampling parameters for inference
#[derive(Clone, Debug, Default)]
pub struct InferenceParams {
    /// Temperature for sampling
    pub temperature: f32,
    /// Top-p (nucleus) sampling
    pub top_p: f32,
    /// Top-k sampling
    pub top_k: u32,
    /// Minimum probability threshold
    pub min_p: f32,
    /// Repetition penalty
    pub repetition_penalty: f32,
    /// Maximum tokens to generate
    pub max_tokens: u32,
    /// Stop sequences
    pub stop_sequences: alloc::vec::Vec<String>,
    /// Random seed (None for random)
    pub seed: Option<u64>,
}

impl InferenceParams {
    /// Greedy decoding (temperature = 0)
    pub fn greedy() -> Self {
        Self {
            temperature: 0.0,
            max_tokens: 256,
            ..Default::default()
        }
    }

    /// Balanced sampling
    pub fn balanced() -> Self {
        Self {
            temperature: 0.7,
            top_p: 0.9,
            top_k: 40,
            max_tokens: 512,
            ..Default::default()
        }
    }

    /// Creative sampling
    pub fn creative() -> Self {
        Self {
            temperature: 1.0,
            top_p: 0.95,
            top_k: 100,
            max_tokens: 1024,
            ..Default::default()
        }
    }
}

/// Request execution state
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RequestState {
    /// Queued, waiting to be processed
    Queued,
    /// Prefilling (processing input)
    Prefilling,
    /// Generating (autoregressive decoding)
    Generating,
    /// Completed successfully
    Completed,
    /// Failed with error
    Failed,
    /// Cancelled by user
    Cancelled,
}

/// Inference statistics
#[derive(Clone, Debug, Default)]
pub struct InferenceStats {
    /// Total requests processed
    pub total_requests: u64,
    /// Total input tokens processed
    pub total_input_tokens: u64,
    /// Total output tokens generated
    pub total_output_tokens: u64,
    /// Total prefill time (microseconds)
    pub total_prefill_time_us: u64,
    /// Total generation time (microseconds)
    pub total_generation_time_us: u64,
    /// Average time to first token (microseconds)
    pub avg_ttft_us: u64,
    /// Average inter-token latency (microseconds)
    pub avg_itl_us: u64,
}

impl InferenceContext {
    /// Create a new inference context
    pub fn new(model_id: ObjectId, config: InferenceConfig) -> Result<Self, super::TensorError> {
        Ok(Self {
            model_id,
            config,
            pending: VecDeque::new(),
            stats: InferenceStats::default(),
        })
    }

    /// Submit an inference request
    pub fn submit(
        &self,
        input: ObjectId,
        params: InferenceParams,
    ) -> Result<u64, super::TensorError> {
        let request_id = NEXT_REQUEST_ID.fetch_add(1, Ordering::Relaxed);

        let request = InferenceRequest {
            id: request_id,
            input,
            params,
            state: RequestState::Queued,
        };

        // TODO: Actually queue the request
        // For now, just return the ID
        let _ = request;

        Ok(request_id)
    }

    /// Get current queue depth
    pub fn queue_depth(&self) -> usize {
        self.pending.len()
    }

    /// Get throughput statistics
    pub fn tokens_per_second(&self) -> f64 {
        if self.stats.total_generation_time_us == 0 {
            return 0.0;
        }

        (self.stats.total_output_tokens as f64 * 1_000_000.0)
            / self.stats.total_generation_time_us as f64
    }
}

/// Inference scheduler for fair GPU/NPU time allocation
pub struct InferenceScheduler {
    /// Contexts with pending work
    active_contexts: alloc::vec::Vec<ObjectId>,
    /// Current scheduling quantum (microseconds)
    quantum_us: u64,
}

impl InferenceScheduler {
    /// Create a new scheduler
    pub fn new(quantum_us: u64) -> Self {
        Self {
            active_contexts: alloc::vec::Vec::new(),
            quantum_us,
        }
    }

    /// Add context to scheduling
    pub fn add_context(&mut self, context_id: ObjectId) {
        if !self.active_contexts.contains(&context_id) {
            self.active_contexts.push(context_id);
        }
    }

    /// Remove context from scheduling
    pub fn remove_context(&mut self, context_id: ObjectId) {
        self.active_contexts.retain(|&id| id != context_id);
    }

    /// Get next context to run (round-robin for now)
    pub fn next(&mut self) -> Option<ObjectId> {
        if self.active_contexts.is_empty() {
            return None;
        }

        // Round-robin: move first to end and return it
        let context = self.active_contexts.remove(0);
        self.active_contexts.push(context);
        Some(context)
    }
}
