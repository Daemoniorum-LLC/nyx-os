//! Settings migration between versions

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::path::PathBuf;

/// Migration manager
pub struct MigrationManager {
    migrations: Vec<Migration>,
    current_version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Migration {
    pub from_version: String,
    pub to_version: String,
    pub description: String,
    #[serde(default)]
    pub operations: Vec<MigrationOp>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum MigrationOp {
    Rename { from: String, to: String },
    Delete { path: String },
    SetDefault { path: String, value: Value },
    Transform { path: String, transformer: String },
    Move { from: String, to: String },
    Copy { from: String, to: String },
    Merge { sources: Vec<String>, target: String },
}

impl MigrationManager {
    pub fn new(current_version: &str) -> Self {
        Self {
            migrations: Vec::new(),
            current_version: current_version.to_string(),
        }
    }

    /// Register a migration
    pub fn register(&mut self, migration: Migration) {
        self.migrations.push(migration);
    }

    /// Load migrations from directory
    pub async fn load_migrations(&mut self, dir: &PathBuf) -> Result<()> {
        let mut entries = tokio::fs::read_dir(dir).await?;

        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();

            if path.extension().map(|e| e == "yaml" || e == "yml").unwrap_or(false) {
                let content = tokio::fs::read_to_string(&path).await?;
                let migration: Migration = serde_yaml::from_str(&content)?;
                self.register(migration);
            }
        }

        // Sort migrations by version
        self.migrations.sort_by(|a, b| {
            version_compare(&a.from_version, &b.from_version)
        });

        Ok(())
    }

    /// Get migrations needed to reach current version
    pub fn get_migration_path(&self, from_version: &str) -> Vec<&Migration> {
        let mut path = Vec::new();
        let mut current = from_version.to_string();

        while current != self.current_version {
            if let Some(migration) = self.migrations.iter()
                .find(|m| m.from_version == current)
            {
                path.push(migration);
                current = migration.to_version.clone();
            } else {
                break;
            }
        }

        path
    }

    /// Apply migrations to settings
    pub async fn migrate(
        &self,
        settings: &mut HashMap<String, Value>,
        from_version: &str,
    ) -> Result<MigrationResult> {
        let migrations = self.get_migration_path(from_version);

        if migrations.is_empty() {
            return Ok(MigrationResult {
                from_version: from_version.to_string(),
                to_version: from_version.to_string(),
                operations_applied: 0,
                warnings: Vec::new(),
            });
        }

        let mut result = MigrationResult {
            from_version: from_version.to_string(),
            to_version: self.current_version.clone(),
            operations_applied: 0,
            warnings: Vec::new(),
        };

        for migration in migrations {
            tracing::info!(
                "Applying migration: {} -> {} ({})",
                migration.from_version,
                migration.to_version,
                migration.description
            );

            for op in &migration.operations {
                match self.apply_operation(settings, op) {
                    Ok(()) => result.operations_applied += 1,
                    Err(e) => result.warnings.push(format!(
                        "Failed to apply {:?}: {}",
                        op, e
                    )),
                }
            }
        }

        Ok(result)
    }

    fn apply_operation(
        &self,
        settings: &mut HashMap<String, Value>,
        op: &MigrationOp,
    ) -> Result<()> {
        match op {
            MigrationOp::Rename { from, to } => {
                if let Some(value) = settings.remove(from) {
                    settings.insert(to.clone(), value);
                }
            }

            MigrationOp::Delete { path } => {
                settings.remove(path);
            }

            MigrationOp::SetDefault { path, value } => {
                if !settings.contains_key(path) {
                    settings.insert(path.clone(), value.clone());
                }
            }

            MigrationOp::Transform { path, transformer } => {
                if let Some(value) = settings.get_mut(path) {
                    *value = apply_transformer(value, transformer)?;
                }
            }

            MigrationOp::Move { from, to } => {
                if let Some(value) = settings.remove(from) {
                    settings.insert(to.clone(), value);
                }
            }

            MigrationOp::Copy { from, to } => {
                if let Some(value) = settings.get(from).cloned() {
                    settings.insert(to.clone(), value);
                }
            }

            MigrationOp::Merge { sources, target } => {
                let mut merged = serde_json::Map::new();

                for source in sources {
                    if let Some(Value::Object(obj)) = settings.get(source) {
                        for (k, v) in obj {
                            merged.insert(k.clone(), v.clone());
                        }
                    }
                }

                if !merged.is_empty() {
                    settings.insert(target.clone(), Value::Object(merged));
                }

                // Remove sources after merging
                for source in sources {
                    settings.remove(source);
                }
            }
        }

        Ok(())
    }

    /// Create a backup before migration
    pub async fn backup(&self, settings: &HashMap<String, Value>, backup_dir: &PathBuf) -> Result<PathBuf> {
        tokio::fs::create_dir_all(backup_dir).await?;

        let timestamp = chrono::Utc::now().format("%Y%m%d_%H%M%S");
        let backup_path = backup_dir.join(format!("settings_backup_{}.yaml", timestamp));

        let content = serde_yaml::to_string(settings)?;
        tokio::fs::write(&backup_path, content).await?;

        tracing::info!("Created backup: {:?}", backup_path);
        Ok(backup_path)
    }

    /// Restore from backup
    pub async fn restore(&self, backup_path: &PathBuf) -> Result<HashMap<String, Value>> {
        let content = tokio::fs::read_to_string(backup_path).await?;
        let settings: HashMap<String, Value> = serde_yaml::from_str(&content)?;
        Ok(settings)
    }
}

#[derive(Debug, Clone)]
pub struct MigrationResult {
    pub from_version: String,
    pub to_version: String,
    pub operations_applied: usize,
    pub warnings: Vec<String>,
}

/// Apply a value transformer
fn apply_transformer(value: &Value, transformer: &str) -> Result<Value> {
    match transformer {
        "to_string" => Ok(Value::String(value.to_string())),

        "to_number" => {
            if let Some(s) = value.as_str() {
                if let Ok(n) = s.parse::<i64>() {
                    return Ok(Value::Number(n.into()));
                }
                if let Ok(n) = s.parse::<f64>() {
                    return Ok(serde_json::Number::from_f64(n)
                        .map(Value::Number)
                        .unwrap_or(Value::Null));
                }
            }
            Err(anyhow!("Cannot convert to number"))
        }

        "to_bool" => {
            let result = match value {
                Value::Bool(b) => *b,
                Value::String(s) => matches!(s.to_lowercase().as_str(), "true" | "yes" | "1"),
                Value::Number(n) => n.as_i64().map(|i| i != 0).unwrap_or(false),
                _ => false,
            };
            Ok(Value::Bool(result))
        }

        "to_array" => {
            if value.is_array() {
                Ok(value.clone())
            } else {
                Ok(Value::Array(vec![value.clone()]))
            }
        }

        "lowercase" => {
            if let Some(s) = value.as_str() {
                Ok(Value::String(s.to_lowercase()))
            } else {
                Ok(value.clone())
            }
        }

        "uppercase" => {
            if let Some(s) = value.as_str() {
                Ok(Value::String(s.to_uppercase()))
            } else {
                Ok(value.clone())
            }
        }

        "trim" => {
            if let Some(s) = value.as_str() {
                Ok(Value::String(s.trim().to_string()))
            } else {
                Ok(value.clone())
            }
        }

        _ => Err(anyhow!("Unknown transformer: {}", transformer)),
    }
}

/// Compare semantic versions
fn version_compare(a: &str, b: &str) -> std::cmp::Ordering {
    let parse = |s: &str| -> Vec<u32> {
        s.split('.')
            .filter_map(|p| p.parse().ok())
            .collect()
    };

    let va = parse(a);
    let vb = parse(b);

    for i in 0..va.len().max(vb.len()) {
        let pa = va.get(i).unwrap_or(&0);
        let pb = vb.get(i).unwrap_or(&0);

        match pa.cmp(pb) {
            std::cmp::Ordering::Equal => continue,
            other => return other,
        }
    }

    std::cmp::Ordering::Equal
}

/// Migration builder for programmatic migrations
pub struct MigrationBuilder {
    from_version: String,
    to_version: String,
    description: String,
    operations: Vec<MigrationOp>,
}

impl MigrationBuilder {
    pub fn new(from_version: &str, to_version: &str) -> Self {
        Self {
            from_version: from_version.to_string(),
            to_version: to_version.to_string(),
            description: String::new(),
            operations: Vec::new(),
        }
    }

    pub fn description(mut self, desc: &str) -> Self {
        self.description = desc.to_string();
        self
    }

    pub fn rename(mut self, from: &str, to: &str) -> Self {
        self.operations.push(MigrationOp::Rename {
            from: from.to_string(),
            to: to.to_string(),
        });
        self
    }

    pub fn delete(mut self, path: &str) -> Self {
        self.operations.push(MigrationOp::Delete {
            path: path.to_string(),
        });
        self
    }

    pub fn set_default(mut self, path: &str, value: Value) -> Self {
        self.operations.push(MigrationOp::SetDefault {
            path: path.to_string(),
            value,
        });
        self
    }

    pub fn transform(mut self, path: &str, transformer: &str) -> Self {
        self.operations.push(MigrationOp::Transform {
            path: path.to_string(),
            transformer: transformer.to_string(),
        });
        self
    }

    pub fn build(self) -> Migration {
        Migration {
            from_version: self.from_version,
            to_version: self.to_version,
            description: self.description,
            operations: self.operations,
        }
    }
}
