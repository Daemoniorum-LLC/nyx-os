//! Ritual storage and execution management

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, Result};
use grimoire_core::{Ritual, RitualId, RitualExecution, ExecutionStatus};
use tracing::{info, warn, debug};
use uuid::Uuid;

/// Ritual store managing all registered rituals
pub struct RitualStore {
    /// Loaded rituals
    rituals: HashMap<RitualId, Ritual>,
    /// Active executions
    executions: HashMap<Uuid, RitualExecution>,
    /// Rituals directory
    rituals_dir: PathBuf,
}

impl RitualStore {
    /// Create a new ritual store
    pub fn new(rituals_dir: &Path) -> Self {
        Self {
            rituals: HashMap::new(),
            executions: HashMap::new(),
            rituals_dir: rituals_dir.to_path_buf(),
        }
    }

    /// Initialize the store
    pub async fn init(&mut self) -> Result<()> {
        // Create directory if needed
        tokio::fs::create_dir_all(&self.rituals_dir).await?;

        // Load rituals from disk
        self.load_rituals().await?;

        info!("RitualStore initialized: {} rituals loaded", self.rituals.len());
        Ok(())
    }

    /// Load rituals from the rituals directory
    async fn load_rituals(&mut self) -> Result<()> {
        let mut entries = match tokio::fs::read_dir(&self.rituals_dir).await {
            Ok(entries) => entries,
            Err(_) => return Ok(()), // Directory doesn't exist yet
        };

        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();

            if path.extension().map(|e| e == "ritual").unwrap_or(false) {
                match self.load_ritual_file(&path).await {
                    Ok(ritual) => {
                        debug!("Loaded ritual: {} from {:?}", ritual.name, path);
                        self.rituals.insert(ritual.id, ritual);
                    }
                    Err(e) => {
                        warn!("Failed to load ritual from {:?}: {}", path, e);
                    }
                }
            }
        }

        Ok(())
    }

    /// Load a single ritual file
    async fn load_ritual_file(&self, path: &Path) -> Result<Ritual> {
        let content = tokio::fs::read_to_string(path).await?;
        Ritual::from_toml(&content).map_err(|e| anyhow!("Parse error: {}", e))
    }

    // ========== Ritual Operations ==========

    /// List all rituals
    pub fn list_rituals(&self) -> Vec<Ritual> {
        self.rituals.values().cloned().collect()
    }

    /// List rituals for a specific persona
    pub fn list_persona_rituals(&self, persona_id: grimoire_core::PersonaId) -> Vec<Ritual> {
        self.rituals
            .values()
            .filter(|r| r.persona_id == persona_id)
            .cloned()
            .collect()
    }

    /// Get a ritual by ID
    pub fn get_ritual(&self, id: RitualId) -> Option<Ritual> {
        self.rituals.get(&id).cloned()
    }

    /// Get a ritual by name
    pub fn get_ritual_by_name(&self, name: &str) -> Option<Ritual> {
        let name_lower = name.to_lowercase();
        self.rituals
            .values()
            .find(|r| r.name.to_lowercase() == name_lower)
            .cloned()
    }

    /// Register a new ritual
    pub async fn register_ritual(&mut self, ritual: Ritual) -> Result<RitualId> {
        let id = ritual.id;

        // Check if already exists
        if self.rituals.contains_key(&id) {
            return Err(anyhow!("Ritual already exists: {}", id));
        }

        // Save to disk
        self.save_ritual(&ritual).await?;

        // Add to memory
        self.rituals.insert(id, ritual);

        info!("Registered new ritual: {}", id);
        Ok(id)
    }

    /// Remove a ritual
    pub async fn remove_ritual(&mut self, id: RitualId) -> Result<()> {
        // Check if exists
        let ritual = self.rituals.get(&id)
            .ok_or_else(|| anyhow!("Ritual not found: {}", id))?
            .clone();

        // Remove from disk
        let path = self.ritual_path(&ritual);
        if path.exists() {
            tokio::fs::remove_file(&path).await?;
        }

        // Remove from memory
        self.rituals.remove(&id);

        info!("Removed ritual: {}", id);
        Ok(())
    }

    /// Save a ritual to disk
    async fn save_ritual(&self, ritual: &Ritual) -> Result<()> {
        let path = self.ritual_path(ritual);
        let content = ritual.to_toml().map_err(|e| anyhow!("{}", e))?;
        tokio::fs::write(&path, content).await?;
        Ok(())
    }

    /// Get the file path for a ritual
    fn ritual_path(&self, ritual: &Ritual) -> PathBuf {
        self.rituals_dir.join(format!(
            "{}.ritual",
            ritual.name.to_lowercase().replace(' ', "_")
        ))
    }

    // ========== Execution Operations ==========

    /// Start executing a ritual
    pub fn start_execution(
        &mut self,
        ritual_id: RitualId,
        parameters: HashMap<String, serde_json::Value>,
    ) -> Result<Uuid> {
        // Check if ritual exists
        if !self.rituals.contains_key(&ritual_id) {
            return Err(anyhow!("Ritual not found: {}", ritual_id));
        }

        // Create execution
        let mut execution = RitualExecution::new(ritual_id);
        execution.variables = parameters;
        execution.status = ExecutionStatus::Running;

        let execution_id = execution.id;
        self.executions.insert(execution_id, execution);

        info!("Started ritual execution: {}", execution_id);
        Ok(execution_id)
    }

    /// Get ritual execution status
    pub fn get_execution(&self, execution_id: Uuid) -> Option<RitualExecution> {
        self.executions.get(&execution_id).cloned()
    }

    /// Update execution status
    pub fn update_execution(
        &mut self,
        execution_id: Uuid,
        status: ExecutionStatus,
        result: Option<serde_json::Value>,
        error: Option<String>,
    ) -> Result<()> {
        let execution = self.executions.get_mut(&execution_id)
            .ok_or_else(|| anyhow!("Execution not found: {}", execution_id))?;

        execution.status = status;
        execution.result = result;
        execution.error = error;

        if matches!(status, ExecutionStatus::Completed | ExecutionStatus::Failed | ExecutionStatus::Cancelled) {
            execution.ended_at = Some(chrono::Utc::now());
        }

        Ok(())
    }

    /// Cancel a running ritual
    pub fn cancel_execution(&mut self, execution_id: Uuid) -> Result<()> {
        let execution = self.executions.get_mut(&execution_id)
            .ok_or_else(|| anyhow!("Execution not found: {}", execution_id))?;

        if execution.status != ExecutionStatus::Running && execution.status != ExecutionStatus::Paused {
            return Err(anyhow!("Execution is not running: {}", execution_id));
        }

        execution.status = ExecutionStatus::Cancelled;
        execution.ended_at = Some(chrono::Utc::now());

        info!("Cancelled ritual execution: {}", execution_id);
        Ok(())
    }

    /// List active executions
    pub fn list_active_executions(&self) -> Vec<RitualExecution> {
        self.executions
            .values()
            .filter(|e| matches!(e.status, ExecutionStatus::Running | ExecutionStatus::Paused))
            .cloned()
            .collect()
    }

    /// Clean up completed executions older than specified duration
    pub fn cleanup_old_executions(&mut self, max_age: std::time::Duration) {
        let cutoff = chrono::Utc::now() - chrono::Duration::from_std(max_age).unwrap();

        self.executions.retain(|_, e| {
            if let Some(ended_at) = e.ended_at {
                ended_at > cutoff
            } else {
                true // Keep running executions
            }
        });
    }

    // ========== Statistics ==========

    /// Get ritual count
    pub fn ritual_count(&self) -> usize {
        self.rituals.len()
    }

    /// Get active execution count
    pub fn active_execution_count(&self) -> usize {
        self.executions
            .values()
            .filter(|e| matches!(e.status, ExecutionStatus::Running | ExecutionStatus::Paused))
            .count()
    }
}
