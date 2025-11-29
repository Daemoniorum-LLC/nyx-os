//! Energy-aware scheduling

/// Energy mode
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EnergyMode {
    /// Maximum performance
    Performance,
    /// Balanced
    Balanced,
    /// Power saver
    PowerSaver,
}

/// Core type (for heterogeneous CPUs)
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CoreType {
    /// Performance core (P-core / big)
    Performance,
    /// Efficiency core (E-core / LITTLE)
    Efficiency,
}

/// Energy hint for threads
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EnergyHint {
    /// Latency-sensitive (prefer P-cores)
    LatencySensitive,
    /// Background work (prefer E-cores)
    Background,
    /// Batch processing (can migrate)
    Batch,
    /// AI inference workload
    Inference,
}

/// CPU topology information
pub struct CpuTopology {
    /// Performance cores
    pub p_cores: alloc::vec::Vec<u32>,
    /// Efficiency cores
    pub e_cores: alloc::vec::Vec<u32>,
    /// SMT siblings (hyperthreads)
    pub smt_siblings: alloc::collections::BTreeMap<u32, u32>,
}

impl CpuTopology {
    /// Get appropriate cores for energy hint
    pub fn cores_for_hint(&self, hint: EnergyHint, mode: EnergyMode) -> &[u32] {
        match (hint, mode) {
            // Latency-sensitive always gets P-cores (if available)
            (EnergyHint::LatencySensitive, _) => {
                if self.p_cores.is_empty() {
                    &self.e_cores
                } else {
                    &self.p_cores
                }
            }

            // Background always gets E-cores (if available)
            (EnergyHint::Background, _) => {
                if self.e_cores.is_empty() {
                    &self.p_cores
                } else {
                    &self.e_cores
                }
            }

            // Batch and Inference depend on mode
            (_, EnergyMode::Performance) => {
                if self.p_cores.is_empty() {
                    &self.e_cores
                } else {
                    &self.p_cores
                }
            }

            (_, EnergyMode::PowerSaver) => {
                if self.e_cores.is_empty() {
                    &self.p_cores
                } else {
                    &self.e_cores
                }
            }

            // Balanced: prefer P-cores but allow E-cores
            (_, EnergyMode::Balanced) => {
                if self.p_cores.is_empty() {
                    &self.e_cores
                } else {
                    &self.p_cores
                }
            }
        }
    }
}
