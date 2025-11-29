//! Freedesktop .desktop file parser

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Desktop entry representation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DesktopEntry {
    pub id: String,
    pub name: String,
    pub generic_name: Option<String>,
    pub comment: Option<String>,
    pub icon: Option<String>,
    pub exec: String,
    pub try_exec: Option<String>,
    pub path: Option<String>,
    pub terminal: bool,
    pub no_display: bool,
    pub hidden: bool,
    pub categories: Vec<String>,
    pub keywords: Vec<String>,
    pub mime_types: Vec<String>,
    pub actions: Vec<DesktopAction>,
    pub startup_notify: bool,
    pub startup_wm_class: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DesktopAction {
    pub id: String,
    pub name: String,
    pub icon: Option<String>,
    pub exec: String,
}

impl DesktopEntry {
    /// Parse a .desktop file
    pub fn parse(path: &Path) -> Result<Self> {
        let content = std::fs::read_to_string(path)?;
        Self::parse_content(&content, path)
    }

    /// Parse desktop file content
    pub fn parse_content(content: &str, path: &Path) -> Result<Self> {
        let mut sections: HashMap<String, HashMap<String, String>> = HashMap::new();
        let mut current_section = String::new();

        for line in content.lines() {
            let line = line.trim();

            // Skip comments and empty lines
            if line.is_empty() || line.starts_with('#') {
                continue;
            }

            // Section header
            if line.starts_with('[') && line.ends_with(']') {
                current_section = line[1..line.len() - 1].to_string();
                sections.entry(current_section.clone()).or_default();
                continue;
            }

            // Key-value pair
            if let Some((key, value)) = line.split_once('=') {
                if let Some(section) = sections.get_mut(&current_section) {
                    section.insert(key.to_string(), value.to_string());
                }
            }
        }

        // Parse main desktop entry
        let entry = sections.get("Desktop Entry")
            .ok_or_else(|| anyhow!("No [Desktop Entry] section"))?;

        // Entry type must be Application
        let entry_type = entry.get("Type").map(|s| s.as_str()).unwrap_or("");
        if entry_type != "Application" {
            return Err(anyhow!("Not an application: {}", entry_type));
        }

        let name = entry.get("Name")
            .ok_or_else(|| anyhow!("Missing Name field"))?
            .clone();

        let exec = entry.get("Exec")
            .ok_or_else(|| anyhow!("Missing Exec field"))?
            .clone();

        // Generate ID from filename
        let id = path.file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("unknown")
            .to_string();

        // Parse actions
        let mut actions = Vec::new();
        let action_ids: Vec<&str> = entry.get("Actions")
            .map(|s| s.split(';').filter(|s| !s.is_empty()).collect())
            .unwrap_or_default();

        for action_id in action_ids {
            let section_name = format!("Desktop Action {}", action_id);
            if let Some(action_section) = sections.get(&section_name) {
                if let Some(action_name) = action_section.get("Name") {
                    if let Some(action_exec) = action_section.get("Exec") {
                        actions.push(DesktopAction {
                            id: action_id.to_string(),
                            name: action_name.clone(),
                            icon: action_section.get("Icon").cloned(),
                            exec: action_exec.clone(),
                        });
                    }
                }
            }
        }

        Ok(DesktopEntry {
            id,
            name,
            generic_name: entry.get("GenericName").cloned(),
            comment: entry.get("Comment").cloned(),
            icon: entry.get("Icon").cloned(),
            exec,
            try_exec: entry.get("TryExec").cloned(),
            path: entry.get("Path").cloned(),
            terminal: entry.get("Terminal").map(|s| s == "true").unwrap_or(false),
            no_display: entry.get("NoDisplay").map(|s| s == "true").unwrap_or(false),
            hidden: entry.get("Hidden").map(|s| s == "true").unwrap_or(false),
            categories: entry.get("Categories")
                .map(|s| s.split(';').filter(|s| !s.is_empty()).map(String::from).collect())
                .unwrap_or_default(),
            keywords: entry.get("Keywords")
                .map(|s| s.split(';').filter(|s| !s.is_empty()).map(String::from).collect())
                .unwrap_or_default(),
            mime_types: entry.get("MimeType")
                .map(|s| s.split(';').filter(|s| !s.is_empty()).map(String::from).collect())
                .unwrap_or_default(),
            actions,
            startup_notify: entry.get("StartupNotify").map(|s| s == "true").unwrap_or(false),
            startup_wm_class: entry.get("StartupWMClass").cloned(),
        })
    }

    /// Check if application should be shown
    pub fn should_display(&self) -> bool {
        !self.no_display && !self.hidden
    }

    /// Get the command with field codes expanded
    pub fn get_command(&self, files: &[String]) -> String {
        expand_exec(&self.exec, files, &self.name, &self.icon)
    }

    /// Get action command
    pub fn get_action_command(&self, action_id: &str, files: &[String]) -> Option<String> {
        self.actions.iter()
            .find(|a| a.id == action_id)
            .map(|a| expand_exec(&a.exec, files, &self.name, &self.icon))
    }
}

/// Expand field codes in Exec string
fn expand_exec(exec: &str, files: &[String], name: &str, icon: &Option<String>) -> String {
    let mut result = exec.to_string();

    // %f - single file
    if let Some(file) = files.first() {
        result = result.replace("%f", &shell_quote(file));
    } else {
        result = result.replace("%f", "");
    }

    // %F - file list
    let file_list: String = files.iter()
        .map(|f| shell_quote(f))
        .collect::<Vec<_>>()
        .join(" ");
    result = result.replace("%F", &file_list);

    // %u - single URL
    if let Some(url) = files.first() {
        result = result.replace("%u", &shell_quote(url));
    } else {
        result = result.replace("%u", "");
    }

    // %U - URL list
    result = result.replace("%U", &file_list);

    // %i - icon
    if let Some(ref icon) = icon {
        result = result.replace("%i", &format!("--icon {}", shell_quote(icon)));
    } else {
        result = result.replace("%i", "");
    }

    // %c - translated name
    result = result.replace("%c", &shell_quote(name));

    // %k - desktop file location (not available in this context)
    result = result.replace("%k", "");

    // %% - literal %
    result = result.replace("%%", "%");

    // Clean up extra whitespace
    result.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn shell_quote(s: &str) -> String {
    if s.contains(char::is_whitespace) || s.contains('\'') || s.contains('"') {
        format!("'{}'", s.replace('\'', "'\"'\"'"))
    } else {
        s.to_string()
    }
}

/// Scan directories for desktop files
pub async fn scan_directories(dirs: &[PathBuf]) -> Vec<(DesktopEntry, PathBuf)> {
    let mut entries = Vec::new();

    for dir in dirs {
        if !dir.exists() {
            continue;
        }

        if let Ok(mut dir_entries) = tokio::fs::read_dir(dir).await {
            while let Ok(Some(entry)) = dir_entries.next_entry().await {
                let path = entry.path();

                if path.extension().map(|e| e == "desktop").unwrap_or(false) {
                    match DesktopEntry::parse(&path) {
                        Ok(desktop_entry) => {
                            if desktop_entry.should_display() {
                                entries.push((desktop_entry, path));
                            }
                        }
                        Err(e) => {
                            tracing::debug!("Failed to parse {:?}: {}", path, e);
                        }
                    }
                }
            }
        }
    }

    entries
}

/// Find icon path for an icon name
pub fn find_icon(name: &str, size: u32) -> Option<PathBuf> {
    // If it's already an absolute path
    if name.starts_with('/') {
        let path = PathBuf::from(name);
        if path.exists() {
            return Some(path);
        }
    }

    // Search in icon theme directories
    let icon_dirs = vec![
        dirs::data_dir().unwrap_or_default().join("icons"),
        PathBuf::from("/usr/share/icons"),
        PathBuf::from("/usr/share/pixmaps"),
    ];

    let themes = vec!["hicolor", "Adwaita", "gnome"];
    let sizes = vec![size, 48, 32, 24, 16, 64, 128, 256];
    let extensions = vec!["png", "svg", "xpm"];

    for icon_dir in &icon_dirs {
        for theme in &themes {
            for &s in &sizes {
                for ext in &extensions {
                    // Standard theme path
                    let path = icon_dir
                        .join(theme)
                        .join(format!("{}x{}", s, s))
                        .join("apps")
                        .join(format!("{}.{}", name, ext));

                    if path.exists() {
                        return Some(path);
                    }

                    // Scalable
                    let path = icon_dir
                        .join(theme)
                        .join("scalable")
                        .join("apps")
                        .join(format!("{}.{}", name, ext));

                    if path.exists() {
                        return Some(path);
                    }
                }
            }
        }

        // Pixmaps
        for ext in &extensions {
            let path = icon_dir.join(format!("{}.{}", name, ext));
            if path.exists() {
                return Some(path);
            }
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_desktop() {
        let content = r#"
[Desktop Entry]
Type=Application
Name=Firefox
GenericName=Web Browser
Exec=firefox %u
Icon=firefox
Categories=Network;WebBrowser;
Keywords=Internet;WWW;Browser;
"#;

        let entry = DesktopEntry::parse_content(content, Path::new("firefox.desktop")).unwrap();
        assert_eq!(entry.name, "Firefox");
        assert!(entry.categories.contains(&"Network".to_string()));
    }

    #[test]
    fn test_expand_exec() {
        let result = expand_exec(
            "app --file %f --name %c",
            &["/path/to/file".to_string()],
            "My App",
            &None,
        );
        assert!(result.contains("/path/to/file"));
        assert!(result.contains("My App"));
    }
}
