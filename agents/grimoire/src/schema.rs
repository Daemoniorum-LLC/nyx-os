//! Settings schema validation

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

/// Schema definition for settings
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SettingsSchema {
    pub name: String,
    pub version: String,
    #[serde(default)]
    pub settings: HashMap<String, SettingDefinition>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SettingDefinition {
    #[serde(rename = "type")]
    pub value_type: ValueType,
    pub description: String,
    #[serde(default)]
    pub default: Option<Value>,
    #[serde(default)]
    pub constraints: Option<Constraints>,
    #[serde(default)]
    pub ui: Option<UiHints>,
    #[serde(default)]
    pub deprecated: bool,
    #[serde(default)]
    pub migration: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ValueType {
    String,
    Integer,
    Float,
    Boolean,
    Array(Box<ValueType>),
    Object,
    Enum(Vec<String>),
    Path,
    Color,
    Keybinding,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Constraints {
    #[serde(default)]
    pub min: Option<f64>,
    #[serde(default)]
    pub max: Option<f64>,
    #[serde(default)]
    pub min_length: Option<usize>,
    #[serde(default)]
    pub max_length: Option<usize>,
    #[serde(default)]
    pub pattern: Option<String>,
    #[serde(default)]
    pub required: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UiHints {
    #[serde(default)]
    pub label: Option<String>,
    #[serde(default)]
    pub category: Option<String>,
    #[serde(default)]
    pub order: Option<i32>,
    #[serde(default)]
    pub hidden: bool,
    #[serde(default)]
    pub widget: Option<String>,
}

/// Schema validator
pub struct SchemaValidator {
    schemas: HashMap<String, SettingsSchema>,
}

impl SchemaValidator {
    pub fn new() -> Self {
        Self {
            schemas: HashMap::new(),
        }
    }

    /// Register a schema
    pub fn register(&mut self, schema: SettingsSchema) {
        self.schemas.insert(schema.name.clone(), schema);
    }

    /// Load schema from file
    pub async fn load_schema(&mut self, path: &std::path::Path) -> Result<()> {
        let content = tokio::fs::read_to_string(path).await?;
        let schema: SettingsSchema = serde_yaml::from_str(&content)?;
        self.register(schema);
        Ok(())
    }

    /// Validate a value against a path
    pub fn validate(&self, schema_name: &str, path: &str, value: &Value) -> ValidationResult {
        let schema = match self.schemas.get(schema_name) {
            Some(s) => s,
            None => return ValidationResult::error("Unknown schema"),
        };

        let definition = match schema.settings.get(path) {
            Some(d) => d,
            None => return ValidationResult::warning("Setting not in schema"),
        };

        if definition.deprecated {
            return ValidationResult::warning("Setting is deprecated");
        }

        self.validate_value(value, &definition.value_type, &definition.constraints)
    }

    fn validate_value(
        &self,
        value: &Value,
        expected_type: &ValueType,
        constraints: &Option<Constraints>,
    ) -> ValidationResult {
        // Type check
        let type_valid = match (value, expected_type) {
            (Value::String(_), ValueType::String) => true,
            (Value::String(_), ValueType::Path) => true,
            (Value::String(_), ValueType::Color) => true,
            (Value::String(_), ValueType::Keybinding) => true,
            (Value::String(s), ValueType::Enum(values)) => values.contains(s),
            (Value::Number(n), ValueType::Integer) => n.is_i64(),
            (Value::Number(_), ValueType::Float) => true,
            (Value::Bool(_), ValueType::Boolean) => true,
            (Value::Array(arr), ValueType::Array(inner)) => {
                arr.iter().all(|v| self.validate_value(v, inner, &None).is_valid())
            }
            (Value::Object(_), ValueType::Object) => true,
            _ => false,
        };

        if !type_valid {
            return ValidationResult::error(&format!(
                "Type mismatch: expected {:?}, got {:?}",
                expected_type, value
            ));
        }

        // Constraint checks
        if let Some(c) = &constraints {
            if let Some(err) = self.check_constraints(value, c) {
                return ValidationResult::error(&err);
            }
        }

        ValidationResult::valid()
    }

    fn check_constraints(&self, value: &Value, constraints: &Constraints) -> Option<String> {
        match value {
            Value::Number(n) => {
                if let Some(min) = constraints.min {
                    if n.as_f64().unwrap_or(0.0) < min {
                        return Some(format!("Value below minimum: {}", min));
                    }
                }
                if let Some(max) = constraints.max {
                    if n.as_f64().unwrap_or(0.0) > max {
                        return Some(format!("Value above maximum: {}", max));
                    }
                }
            }
            Value::String(s) => {
                if let Some(min_len) = constraints.min_length {
                    if s.len() < min_len {
                        return Some(format!("String too short: min {}", min_len));
                    }
                }
                if let Some(max_len) = constraints.max_length {
                    if s.len() > max_len {
                        return Some(format!("String too long: max {}", max_len));
                    }
                }
                if let Some(ref pattern) = constraints.pattern {
                    if let Ok(regex) = regex::Regex::new(pattern) {
                        if !regex.is_match(s) {
                            return Some(format!("Value doesn't match pattern: {}", pattern));
                        }
                    }
                }
            }
            Value::Array(arr) => {
                if let Some(min_len) = constraints.min_length {
                    if arr.len() < min_len {
                        return Some(format!("Array too short: min {}", min_len));
                    }
                }
                if let Some(max_len) = constraints.max_length {
                    if arr.len() > max_len {
                        return Some(format!("Array too long: max {}", max_len));
                    }
                }
            }
            _ => {}
        }

        None
    }

    /// Get default value for a setting
    pub fn get_default(&self, schema_name: &str, path: &str) -> Option<Value> {
        self.schemas.get(schema_name)
            .and_then(|s| s.settings.get(path))
            .and_then(|d| d.default.clone())
    }

    /// List all settings in a schema
    pub fn list_settings(&self, schema_name: &str) -> Vec<(String, &SettingDefinition)> {
        self.schemas.get(schema_name)
            .map(|s| s.settings.iter().map(|(k, v)| (k.clone(), v)).collect())
            .unwrap_or_default()
    }

    /// Get settings by category
    pub fn get_by_category(&self, schema_name: &str, category: &str) -> Vec<(String, &SettingDefinition)> {
        self.schemas.get(schema_name)
            .map(|s| {
                s.settings.iter()
                    .filter(|(_, v)| {
                        v.ui.as_ref()
                            .and_then(|ui| ui.category.as_ref())
                            .map(|c| c == category)
                            .unwrap_or(false)
                    })
                    .map(|(k, v)| (k.clone(), v))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Get all categories in a schema
    pub fn get_categories(&self, schema_name: &str) -> Vec<String> {
        let mut categories: Vec<String> = self.schemas.get(schema_name)
            .map(|s| {
                s.settings.values()
                    .filter_map(|v| v.ui.as_ref())
                    .filter_map(|ui| ui.category.clone())
                    .collect()
            })
            .unwrap_or_default();

        categories.sort();
        categories.dedup();
        categories
    }
}

impl Default for SchemaValidator {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone)]
pub struct ValidationResult {
    pub valid: bool,
    pub level: ValidationLevel,
    pub message: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ValidationLevel {
    Valid,
    Warning,
    Error,
}

impl ValidationResult {
    pub fn valid() -> Self {
        Self {
            valid: true,
            level: ValidationLevel::Valid,
            message: None,
        }
    }

    pub fn warning(message: &str) -> Self {
        Self {
            valid: true,
            level: ValidationLevel::Warning,
            message: Some(message.to_string()),
        }
    }

    pub fn error(message: &str) -> Self {
        Self {
            valid: false,
            level: ValidationLevel::Error,
            message: Some(message.to_string()),
        }
    }

    pub fn is_valid(&self) -> bool {
        self.valid
    }
}

/// Generate default schema from struct
#[macro_export]
macro_rules! generate_schema {
    ($name:ident, $($field:ident: $type:ty = $default:expr),* $(,)?) => {
        impl $name {
            pub fn schema() -> SettingsSchema {
                let mut settings = HashMap::new();
                $(
                    settings.insert(
                        stringify!($field).to_string(),
                        SettingDefinition {
                            value_type: <$type as SchemaType>::value_type(),
                            description: String::new(),
                            default: Some(serde_json::to_value($default).unwrap()),
                            constraints: None,
                            ui: None,
                            deprecated: false,
                            migration: None,
                        },
                    );
                )*
                SettingsSchema {
                    name: stringify!($name).to_string(),
                    version: "1.0.0".to_string(),
                    settings,
                }
            }
        }
    };
}

/// Trait for schema type mapping
pub trait SchemaType {
    fn value_type() -> ValueType;
}

impl SchemaType for String {
    fn value_type() -> ValueType { ValueType::String }
}

impl SchemaType for i64 {
    fn value_type() -> ValueType { ValueType::Integer }
}

impl SchemaType for f64 {
    fn value_type() -> ValueType { ValueType::Float }
}

impl SchemaType for bool {
    fn value_type() -> ValueType { ValueType::Boolean }
}

impl<T: SchemaType> SchemaType for Vec<T> {
    fn value_type() -> ValueType {
        ValueType::Array(Box::new(T::value_type()))
    }
}

/// Schema registry that loads and manages schemas
pub struct SchemaRegistry {
    validator: SchemaValidator,
}

impl SchemaRegistry {
    /// Create a new registry and load schemas from a directory
    pub fn new(schemas_dir: &std::path::Path) -> Result<Self> {
        let mut registry = Self {
            validator: SchemaValidator::new(),
        };

        // Load schemas from directory if it exists
        if schemas_dir.exists() {
            for entry in std::fs::read_dir(schemas_dir)? {
                let entry = entry?;
                let path = entry.path();

                if path.extension().map(|e| e == "yaml" || e == "yml").unwrap_or(false) {
                    let content = std::fs::read_to_string(&path)?;
                    let schema: SettingsSchema = serde_yaml::from_str(&content)?;
                    registry.validator.register(schema);
                }
            }
        }

        Ok(registry)
    }

    /// Create an empty registry (for when schemas can't be loaded)
    pub fn empty() -> Self {
        Self {
            validator: SchemaValidator::new(),
        }
    }

    /// Get the validator
    pub fn validator(&self) -> &SchemaValidator {
        &self.validator
    }
}
