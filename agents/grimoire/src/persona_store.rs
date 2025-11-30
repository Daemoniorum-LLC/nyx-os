//! Persona storage and management
//!
//! Manages personas on disk and in memory, integrating with Cipher
//! for encrypted persona memory persistence.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{anyhow, Result};
use grimoire_core::{
    Persona, PersonaId, PersonaMemory, MemoryEntry,
    builtin, GrimoireError,
};
use tokio::sync::RwLock;
use tracing::{info, warn, error, debug};

/// Persona store managing all registered personas
pub struct PersonaStore {
    /// Loaded personas
    personas: Arc<RwLock<HashMap<PersonaId, Persona>>>,
    /// Persona memory (per-persona)
    memories: Arc<RwLock<HashMap<PersonaId, PersonaMemory>>>,
    /// Personas directory
    personas_dir: PathBuf,
    /// Memory storage directory
    memory_dir: PathBuf,
    /// Whether Cipher integration is available
    cipher_available: bool,
}

impl PersonaStore {
    /// Create a new persona store
    pub fn new(base_dir: &Path) -> Self {
        Self {
            personas: Arc::new(RwLock::new(HashMap::new())),
            memories: Arc::new(RwLock::new(HashMap::new())),
            personas_dir: base_dir.join("personas"),
            memory_dir: base_dir.join("memory"),
            cipher_available: false, // Will be set during init
        }
    }

    /// Initialize the store
    pub async fn init(&self) -> Result<()> {
        // Create directories if needed
        tokio::fs::create_dir_all(&self.personas_dir).await?;
        tokio::fs::create_dir_all(&self.memory_dir).await?;

        // Load built-in personas
        self.load_builtin_personas().await?;

        // Load custom personas from disk
        self.load_custom_personas().await?;

        // Check for Cipher daemon availability
        self.check_cipher_availability().await;

        // Load persisted memories
        self.load_memories().await?;

        info!(
            "PersonaStore initialized: {} personas loaded",
            self.personas.read().await.len()
        );

        Ok(())
    }

    /// Load built-in personas (Lilith, Mammon, Leviathan)
    async fn load_builtin_personas(&self) -> Result<()> {
        let mut personas = self.personas.write().await;

        for persona in builtin::all() {
            debug!("Loading built-in persona: {}", persona.name);
            personas.insert(persona.id, persona);
        }

        Ok(())
    }

    /// Load custom personas from the personas directory
    async fn load_custom_personas(&self) -> Result<()> {
        let mut entries = tokio::fs::read_dir(&self.personas_dir).await?;

        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();

            if path.extension().map(|e| e == "grimoire").unwrap_or(false) {
                match self.load_persona_file(&path).await {
                    Ok(persona) => {
                        info!("Loaded custom persona: {} from {:?}", persona.name, path);
                        self.personas.write().await.insert(persona.id, persona);
                    }
                    Err(e) => {
                        warn!("Failed to load persona from {:?}: {}", path, e);
                    }
                }
            }
        }

        Ok(())
    }

    /// Load a single persona file
    async fn load_persona_file(&self, path: &Path) -> Result<Persona> {
        let content = tokio::fs::read_to_string(path).await?;
        Persona::from_toml(&content).map_err(|e| anyhow!("Parse error: {}", e))
    }

    /// Check if Cipher daemon is available
    async fn check_cipher_availability(&self) {
        // TODO: Actually check for Cipher socket
        // For now, assume not available
        // self.cipher_available = cipher_client::is_available().await;
    }

    /// Load persisted memories
    async fn load_memories(&self) -> Result<()> {
        let mut entries = match tokio::fs::read_dir(&self.memory_dir).await {
            Ok(entries) => entries,
            Err(_) => return Ok(()), // No memory directory yet
        };

        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();

            if path.extension().map(|e| e == "memory").unwrap_or(false) {
                match self.load_memory_file(&path).await {
                    Ok(memory) => {
                        debug!("Loaded memory for persona: {}", memory.persona_id);
                        self.memories.write().await.insert(memory.persona_id, memory);
                    }
                    Err(e) => {
                        warn!("Failed to load memory from {:?}: {}", path, e);
                    }
                }
            }
        }

        Ok(())
    }

    /// Load a single memory file
    async fn load_memory_file(&self, path: &Path) -> Result<PersonaMemory> {
        let content = tokio::fs::read(path).await?;

        // TODO: If Cipher is available, decrypt the content first
        // let decrypted = cipher_client::decrypt(&content).await?;

        PersonaMemory::deserialize(&content)
            .map_err(|e| anyhow!("Parse error: {}", e))
    }

    // ========== Persona Operations ==========

    /// List all personas
    pub async fn list_personas(&self) -> Vec<Persona> {
        self.personas.read().await.values().cloned().collect()
    }

    /// Get a persona by ID
    pub async fn get_persona(&self, id: PersonaId) -> Option<Persona> {
        self.personas.read().await.get(&id).cloned()
    }

    /// Get a persona by name
    pub async fn get_persona_by_name(&self, name: &str) -> Option<Persona> {
        let name_lower = name.to_lowercase();
        self.personas
            .read()
            .await
            .values()
            .find(|p| p.name.to_lowercase() == name_lower)
            .cloned()
    }

    /// Register a new persona
    pub async fn register_persona(&self, persona: Persona) -> Result<PersonaId> {
        let id = persona.id;

        // Check if already exists
        if self.personas.read().await.contains_key(&id) {
            return Err(anyhow!("Persona already exists: {}", id));
        }

        // Save to disk
        self.save_persona(&persona).await?;

        // Add to memory
        self.personas.write().await.insert(id, persona);

        // Initialize empty memory
        self.memories.write().await.insert(id, PersonaMemory::new(id));

        info!("Registered new persona: {}", id);
        Ok(id)
    }

    /// Update an existing persona
    pub async fn update_persona(&self, persona: Persona) -> Result<()> {
        let id = persona.id;

        // Check if exists
        if !self.personas.read().await.contains_key(&id) {
            return Err(anyhow!("Persona not found: {}", id));
        }

        // Don't allow updating built-in personas
        if persona.is_builtin() {
            return Err(anyhow!("Cannot update built-in persona: {}", persona.name));
        }

        // Save to disk
        self.save_persona(&persona).await?;

        // Update in memory
        self.personas.write().await.insert(id, persona);

        info!("Updated persona: {}", id);
        Ok(())
    }

    /// Remove a persona
    pub async fn remove_persona(&self, id: PersonaId) -> Result<()> {
        // Check if exists
        let persona = self.personas.read().await.get(&id).cloned();
        let persona = persona.ok_or_else(|| anyhow!("Persona not found: {}", id))?;

        // Don't allow removing built-in personas
        if persona.is_builtin() {
            return Err(anyhow!("Cannot remove built-in persona: {}", persona.name));
        }

        // Remove from disk
        let path = self.persona_path(&persona);
        if path.exists() {
            tokio::fs::remove_file(&path).await?;
        }

        // Remove memory
        let memory_path = self.memory_path(id);
        if memory_path.exists() {
            tokio::fs::remove_file(&memory_path).await?;
        }

        // Remove from memory
        self.personas.write().await.remove(&id);
        self.memories.write().await.remove(&id);

        info!("Removed persona: {}", id);
        Ok(())
    }

    /// Save a persona to disk
    async fn save_persona(&self, persona: &Persona) -> Result<()> {
        let path = self.persona_path(persona);
        let content = persona.to_toml().map_err(|e| anyhow!("{}", e))?;
        tokio::fs::write(&path, content).await?;
        Ok(())
    }

    /// Get the file path for a persona
    fn persona_path(&self, persona: &Persona) -> PathBuf {
        self.personas_dir.join(format!(
            "{}.grimoire",
            persona.name.to_lowercase().replace(' ', "_")
        ))
    }

    /// Get the memory file path for a persona
    fn memory_path(&self, id: PersonaId) -> PathBuf {
        self.memory_dir.join(format!("{}.memory", id))
    }

    // ========== Memory Operations ==========

    /// Get memory for a persona
    pub async fn get_memory(&self, persona_id: PersonaId) -> Option<PersonaMemory> {
        self.memories.read().await.get(&persona_id).cloned()
    }

    /// Add a memory entry
    pub async fn add_memory(&self, persona_id: PersonaId, entry: MemoryEntry) -> Result<()> {
        let mut memories = self.memories.write().await;

        let memory = memories
            .entry(persona_id)
            .or_insert_with(|| PersonaMemory::new(persona_id));

        memory.remember(entry);
        Ok(())
    }

    /// Recall memories matching a query
    pub async fn recall_memory(
        &self,
        persona_id: PersonaId,
        query: &str,
        limit: usize,
    ) -> Vec<MemoryEntry> {
        let mut memories = self.memories.write().await;

        if let Some(memory) = memories.get_mut(&persona_id) {
            memory.recall(query, limit).into_iter().cloned().collect()
        } else {
            Vec::new()
        }
    }

    /// Clear session memory for a persona
    pub async fn clear_session_memory(&self, persona_id: PersonaId) -> Result<()> {
        let mut memories = self.memories.write().await;

        if let Some(memory) = memories.get_mut(&persona_id) {
            memory.clear_session();
        }

        Ok(())
    }

    /// Clear all memory for a persona
    pub async fn clear_all_memory(&self, persona_id: PersonaId) -> Result<()> {
        let mut memories = self.memories.write().await;

        if let Some(memory) = memories.get_mut(&persona_id) {
            memory.clear_all();
        }

        // Also delete from disk
        let path = self.memory_path(persona_id);
        if path.exists() {
            tokio::fs::remove_file(&path).await?;
        }

        Ok(())
    }

    /// Persist memory to disk
    pub async fn persist_memory(&self, persona_id: PersonaId) -> Result<()> {
        let memories = self.memories.read().await;

        if let Some(memory) = memories.get(&persona_id) {
            let data = memory.serialize().map_err(|e| anyhow!("{}", e))?;

            // TODO: If Cipher is available, encrypt the data
            // let encrypted = cipher_client::encrypt(&data).await?;

            let path = self.memory_path(persona_id);
            tokio::fs::write(&path, &data).await?;

            debug!("Persisted memory for persona: {}", persona_id);
        }

        Ok(())
    }

    /// Persist all memories to disk
    pub async fn persist_all_memories(&self) -> Result<()> {
        let persona_ids: Vec<PersonaId> = self.memories.read().await.keys().cloned().collect();

        for id in persona_ids {
            if let Err(e) = self.persist_memory(id).await {
                warn!("Failed to persist memory for {}: {}", id, e);
            }
        }

        Ok(())
    }

    // ========== Statistics ==========

    /// Get persona count
    pub async fn persona_count(&self) -> usize {
        self.personas.read().await.len()
    }

    /// Check if Cipher is available
    pub fn cipher_available(&self) -> bool {
        self.cipher_available
    }

    /// Get builtin personas
    pub fn get_builtin_personas(&self) -> Vec<Persona> {
        builtin::all()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_persona_store_init() {
        let dir = tempdir().unwrap();
        let store = PersonaStore::new(dir.path());
        store.init().await.unwrap();

        // Should have built-in personas
        let personas = store.list_personas().await;
        assert!(personas.len() >= 3);
    }

    #[tokio::test]
    async fn test_get_persona_by_name() {
        let dir = tempdir().unwrap();
        let store = PersonaStore::new(dir.path());
        store.init().await.unwrap();

        let lilith = store.get_persona_by_name("Lilith").await;
        assert!(lilith.is_some());
        assert_eq!(lilith.unwrap().name, "Lilith");
    }
}
