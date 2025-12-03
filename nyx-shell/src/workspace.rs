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
