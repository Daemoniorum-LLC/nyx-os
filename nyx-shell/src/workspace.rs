//! Workspace management for Nyx Shell

use serde::{Deserialize, Serialize};

/// Workspace identifier
pub type WorkspaceId = u32;

/// Workspace information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Workspace {
    /// Unique ID
    pub id: WorkspaceId,
    /// Display name
    pub name: String,
    /// Is this the active workspace
    pub active: bool,
    /// Number of windows
    pub window_count: usize,
    /// Thumbnail image (optional, base64 encoded)
    pub thumbnail: Option<String>,
}

impl Workspace {
    /// Create a new workspace
    pub fn new(id: WorkspaceId, name: impl Into<String>) -> Self {
        Self {
            id,
            name: name.into(),
            active: false,
            window_count: 0,
            thumbnail: None,
        }
    }
}

/// Workspace manager
#[derive(Debug, Clone)]
pub struct WorkspaceManager {
    /// All workspaces
    workspaces: Vec<Workspace>,
    /// Active workspace ID
    active_id: WorkspaceId,
    /// Dynamic workspace mode
    dynamic: bool,
}

impl Default for WorkspaceManager {
    fn default() -> Self {
        Self::new()
    }
}

impl WorkspaceManager {
    /// Create a new workspace manager
    pub fn new() -> Self {
        let workspaces = vec![
            Workspace::new(1, "Main"),
            Workspace::new(2, "Work"),
            Workspace::new(3, "Development"),
            Workspace::new(4, "Media"),
        ];

        let mut manager = Self {
            workspaces,
            active_id: 1,
            dynamic: true,
        };

        manager.set_active(1);
        manager
    }

    /// Get all workspaces
    pub fn workspaces(&self) -> &[Workspace] {
        &self.workspaces
    }

    /// Get the active workspace
    pub fn active(&self) -> Option<&Workspace> {
        self.workspaces.iter().find(|w| w.id == self.active_id)
    }

    /// Get the active workspace ID
    pub fn active_id(&self) -> WorkspaceId {
        self.active_id
    }

    /// Set the active workspace
    pub fn set_active(&mut self, id: WorkspaceId) {
        self.active_id = id;
        for workspace in &mut self.workspaces {
            workspace.active = workspace.id == id;
        }
    }

    /// Switch to the next workspace
    pub fn next(&mut self) {
        let current_idx = self
            .workspaces
            .iter()
            .position(|w| w.id == self.active_id)
            .unwrap_or(0);

        let next_idx = if current_idx + 1 >= self.workspaces.len() {
            0 // Wrap around
        } else {
            current_idx + 1
        };

        if let Some(workspace) = self.workspaces.get(next_idx) {
            self.set_active(workspace.id);
        }
    }

    /// Switch to the previous workspace
    pub fn previous(&mut self) {
        let current_idx = self
            .workspaces
            .iter()
            .position(|w| w.id == self.active_id)
            .unwrap_or(0);

        let prev_idx = if current_idx == 0 {
            self.workspaces.len().saturating_sub(1) // Wrap around
        } else {
            current_idx - 1
        };

        if let Some(workspace) = self.workspaces.get(prev_idx) {
            self.set_active(workspace.id);
        }
    }

    /// Create a new workspace
    pub fn create(&mut self, name: impl Into<String>) -> WorkspaceId {
        let id = self
            .workspaces
            .iter()
            .map(|w| w.id)
            .max()
            .unwrap_or(0)
            + 1;

        self.workspaces.push(Workspace::new(id, name));
        id
    }

    /// Remove a workspace
    pub fn remove(&mut self, id: WorkspaceId) -> bool {
        if self.workspaces.len() <= 1 {
            return false; // Can't remove last workspace
        }

        if let Some(idx) = self.workspaces.iter().position(|w| w.id == id) {
            self.workspaces.remove(idx);

            // If we removed the active workspace, switch to another
            if self.active_id == id {
                if let Some(workspace) = self.workspaces.first() {
                    self.set_active(workspace.id);
                }
            }

            return true;
        }

        false
    }

    /// Rename a workspace
    pub fn rename(&mut self, id: WorkspaceId, name: impl Into<String>) {
        if let Some(workspace) = self.workspaces.iter_mut().find(|w| w.id == id) {
            workspace.name = name.into();
        }
    }

    /// Update window count for a workspace
    pub fn set_window_count(&mut self, id: WorkspaceId, count: usize) {
        if let Some(workspace) = self.workspaces.iter_mut().find(|w| w.id == id) {
            workspace.window_count = count;
        }
    }

    /// Get workspace count
    pub fn count(&self) -> usize {
        self.workspaces.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_workspace_new() {
        let ws = Workspace::new(1, "Test");
        assert_eq!(ws.id, 1);
        assert_eq!(ws.name, "Test");
        assert!(!ws.active);
        assert_eq!(ws.window_count, 0);
        assert!(ws.thumbnail.is_none());
    }

    #[test]
    fn test_workspace_manager_new() {
        let manager = WorkspaceManager::new();
        assert_eq!(manager.count(), 4);
        assert_eq!(manager.active_id(), 1);
    }

    #[test]
    fn test_workspace_manager_default() {
        let manager = WorkspaceManager::default();
        assert_eq!(manager.count(), 4);
    }

    #[test]
    fn test_workspace_manager_active() {
        let manager = WorkspaceManager::new();
        let active = manager.active().unwrap();
        assert_eq!(active.id, 1);
        assert!(active.active);
    }

    #[test]
    fn test_workspace_manager_set_active() {
        let mut manager = WorkspaceManager::new();
        manager.set_active(2);
        assert_eq!(manager.active_id(), 2);

        let active = manager.active().unwrap();
        assert_eq!(active.id, 2);
        assert!(active.active);

        // Previous workspace should not be active
        let ws1 = manager.workspaces().iter().find(|w| w.id == 1).unwrap();
        assert!(!ws1.active);
    }

    #[test]
    fn test_workspace_manager_next() {
        let mut manager = WorkspaceManager::new();
        assert_eq!(manager.active_id(), 1);

        manager.next();
        assert_eq!(manager.active_id(), 2);

        manager.next();
        assert_eq!(manager.active_id(), 3);

        manager.next();
        assert_eq!(manager.active_id(), 4);

        // Should wrap around
        manager.next();
        assert_eq!(manager.active_id(), 1);
    }

    #[test]
    fn test_workspace_manager_previous() {
        let mut manager = WorkspaceManager::new();
        assert_eq!(manager.active_id(), 1);

        // Should wrap around to last
        manager.previous();
        assert_eq!(manager.active_id(), 4);

        manager.previous();
        assert_eq!(manager.active_id(), 3);

        manager.previous();
        assert_eq!(manager.active_id(), 2);

        manager.previous();
        assert_eq!(manager.active_id(), 1);
    }

    #[test]
    fn test_workspace_manager_create() {
        let mut manager = WorkspaceManager::new();
        let initial_count = manager.count();

        let new_id = manager.create("New Workspace");
        assert_eq!(manager.count(), initial_count + 1);
        assert!(new_id > 4); // Should be greater than existing IDs

        let ws = manager.workspaces().iter().find(|w| w.id == new_id).unwrap();
        assert_eq!(ws.name, "New Workspace");
    }

    #[test]
    fn test_workspace_manager_remove() {
        let mut manager = WorkspaceManager::new();
        let initial_count = manager.count();

        let removed = manager.remove(2);
        assert!(removed);
        assert_eq!(manager.count(), initial_count - 1);

        // Workspace 2 should no longer exist
        assert!(manager.workspaces().iter().find(|w| w.id == 2).is_none());
    }

    #[test]
    fn test_workspace_manager_remove_active() {
        let mut manager = WorkspaceManager::new();
        manager.set_active(2);
        assert_eq!(manager.active_id(), 2);

        manager.remove(2);

        // Should switch to another workspace
        assert_ne!(manager.active_id(), 2);
    }

    #[test]
    fn test_workspace_manager_remove_last_fails() {
        let mut manager = WorkspaceManager::new();

        // Remove all but one
        manager.remove(2);
        manager.remove(3);
        manager.remove(4);
        assert_eq!(manager.count(), 1);

        // Should not be able to remove the last one
        let removed = manager.remove(1);
        assert!(!removed);
        assert_eq!(manager.count(), 1);
    }

    #[test]
    fn test_workspace_manager_remove_nonexistent() {
        let mut manager = WorkspaceManager::new();
        let removed = manager.remove(999);
        assert!(!removed);
    }

    #[test]
    fn test_workspace_manager_rename() {
        let mut manager = WorkspaceManager::new();
        manager.rename(1, "Renamed");

        let ws = manager.workspaces().iter().find(|w| w.id == 1).unwrap();
        assert_eq!(ws.name, "Renamed");
    }

    #[test]
    fn test_workspace_manager_rename_nonexistent() {
        let mut manager = WorkspaceManager::new();
        // Should not panic
        manager.rename(999, "Renamed");
    }

    #[test]
    fn test_workspace_manager_set_window_count() {
        let mut manager = WorkspaceManager::new();
        manager.set_window_count(1, 5);

        let ws = manager.workspaces().iter().find(|w| w.id == 1).unwrap();
        assert_eq!(ws.window_count, 5);
    }

    #[test]
    fn test_workspace_serialization() {
        let ws = Workspace::new(1, "Test");
        let json = serde_json::to_string(&ws).unwrap();
        let parsed: Workspace = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.id, ws.id);
        assert_eq!(parsed.name, ws.name);
    }
}
