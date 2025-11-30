//! Persona definitions and types
//!
//! Personas are AI agents with distinct personalities, capabilities, and
//! privacy settings. They can be used in the Sitra browser or as system
//! agents in DaemonOS.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use uuid::Uuid;

/// Unique identifier for a persona
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct PersonaId(Uuid);

impl PersonaId {
    /// Create a new random persona ID
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    /// Create a deterministic ID from a name
    ///
    /// This ensures built-in personas (lilith, mammon, leviathan) always
    /// have the same ID across installations.
    pub fn from_name(name: &str) -> Self {
        let hash = blake3::hash(name.as_bytes());
        let bytes: [u8; 16] = hash.as_bytes()[..16].try_into().unwrap();
        Self(Uuid::from_bytes(bytes))
    }

    /// Get the underlying UUID
    pub fn as_uuid(&self) -> &Uuid {
        &self.0
    }
}

impl Default for PersonaId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for PersonaId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Complete persona definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Persona {
    /// Unique identifier
    pub id: PersonaId,
    /// Display name (e.g., "Lilith", "Mammon")
    pub name: String,
    /// Semantic version
    pub version: semver::Version,
    /// Short description
    pub description: String,
    /// Visual appearance settings
    pub appearance: PersonaAppearance,
    /// Voice/tone settings
    pub voice: PersonaVoice,
    /// Capability flags
    pub capabilities: PersonaCapabilities,
    /// Privacy settings
    pub privacy: PersonaPrivacy,
    /// Model configuration
    pub model: ModelConfig,
    /// System prompt
    pub system_prompt: String,
    /// Available tools
    pub tools: Vec<String>,
    /// Available rituals
    pub rituals: Vec<String>,
    /// Custom metadata
    #[serde(default)]
    pub metadata: std::collections::HashMap<String, serde_json::Value>,
}

impl Persona {
    /// Load persona from a .grimoire file
    pub fn from_file(path: &std::path::Path) -> Result<Self, crate::GrimoireError> {
        let content = std::fs::read_to_string(path)?;
        Self::from_toml(&content)
    }

    /// Parse persona from TOML string
    pub fn from_toml(content: &str) -> Result<Self, crate::GrimoireError> {
        toml::from_str(content).map_err(|e| crate::GrimoireError::ParseError(e.to_string()))
    }

    /// Serialize persona to TOML
    pub fn to_toml(&self) -> Result<String, crate::GrimoireError> {
        toml::to_string_pretty(self).map_err(|e| crate::GrimoireError::ParseError(e.to_string()))
    }

    /// Check if this persona is a built-in (Lilith, Mammon, Leviathan)
    pub fn is_builtin(&self) -> bool {
        matches!(
            self.name.to_lowercase().as_str(),
            "lilith" | "mammon" | "leviathan"
        )
    }
}

/// Visual appearance settings
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersonaAppearance {
    /// Path to sigil/icon SVG
    pub sigil: Option<PathBuf>,
    /// Primary color (hex)
    pub color_primary: String,
    /// Secondary color (hex)
    pub color_secondary: String,
    /// Path to avatar image
    pub avatar: Option<PathBuf>,
    /// CSS class for theming
    #[serde(default)]
    pub theme_class: Option<String>,
}

impl Default for PersonaAppearance {
    fn default() -> Self {
        Self {
            sigil: None,
            color_primary: "#6366f1".to_string(), // Indigo
            color_secondary: "#4f46e5".to_string(),
            avatar: None,
            theme_class: None,
        }
    }
}

/// Voice and tone settings
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersonaVoice {
    /// Overall tone
    pub tone: Tone,
    /// Formality level
    pub formality: Formality,
    /// Verbosity level
    pub verbosity: Verbosity,
    /// Personality traits (e.g., ["curious", "thorough", "skeptical"])
    pub personality_traits: Vec<String>,
    /// Custom voice instructions
    #[serde(default)]
    pub custom_instructions: Option<String>,
}

impl Default for PersonaVoice {
    fn default() -> Self {
        Self {
            tone: Tone::Neutral,
            formality: Formality::Moderate,
            verbosity: Verbosity::Moderate,
            personality_traits: Vec::new(),
            custom_instructions: None,
        }
    }
}

/// Tone of the persona's responses
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Tone {
    Analytical,
    Practical,
    Cautious,
    Friendly,
    Professional,
    Neutral,
    Playful,
}

impl Default for Tone {
    fn default() -> Self {
        Self::Neutral
    }
}

/// Formality level
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Formality {
    Casual,
    Moderate,
    Formal,
}

impl Default for Formality {
    fn default() -> Self {
        Self::Moderate
    }
}

/// Verbosity level
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Verbosity {
    Concise,
    Moderate,
    Detailed,
}

impl Default for Verbosity {
    fn default() -> Self {
        Self::Moderate
    }
}

/// Capability flags
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersonaCapabilities {
    /// Can browse the web
    pub can_browse: bool,
    /// Can execute rituals (automated workflows)
    pub can_execute_rituals: bool,
    /// Can remember across sessions
    pub can_remember: bool,
    /// Can access files (with restrictions)
    pub can_access_files: bool,
    /// Can execute shell commands (with restrictions)
    pub can_execute_commands: bool,
    /// Maximum context window tokens
    pub max_context_tokens: u32,
    /// Maximum output tokens per response
    pub max_output_tokens: u32,
}

impl Default for PersonaCapabilities {
    fn default() -> Self {
        Self {
            can_browse: true,
            can_execute_rituals: true,
            can_remember: true,
            can_access_files: false,
            can_execute_commands: false,
            max_context_tokens: 8192,
            max_output_tokens: 4096,
        }
    }
}

/// Privacy settings for the persona
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersonaPrivacy {
    /// Network routing mode
    pub routing: RoutingMode,
    /// Memory persistence scope
    pub memory_scope: MemoryScope,
    /// Allow clearnet connections
    pub clearnet_allowed: bool,
    /// Only allow .onion sites
    pub onion_only: bool,
    /// Collect telemetry (never by default)
    #[serde(default)]
    pub telemetry: bool,
}

impl Default for PersonaPrivacy {
    fn default() -> Self {
        Self {
            routing: RoutingMode::Tor,
            memory_scope: MemoryScope::Session,
            clearnet_allowed: true,
            onion_only: false,
            telemetry: false,
        }
    }
}

/// Network routing mode
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RoutingMode {
    /// Route through Tor (default)
    Tor,
    /// Route through I2P
    I2p,
    /// Hybrid Tor + I2P
    Hybrid,
    /// Direct connection (clearnet)
    Direct,
}

impl Default for RoutingMode {
    fn default() -> Self {
        Self::Tor
    }
}

/// Memory persistence scope
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryScope {
    /// Memory cleared after session
    Session,
    /// Memory persisted (encrypted via Cipher)
    Persistent,
    /// No memory at all
    Never,
}

impl Default for MemoryScope {
    fn default() -> Self {
        Self::Session
    }
}

/// Model configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelConfig {
    /// Local model name (e.g., "mistral-7b-instruct")
    pub local_model: Option<String>,
    /// Path to local model file (.gguf)
    pub local_model_path: Option<PathBuf>,
    /// Remote provider (e.g., "anthropic", "openai")
    pub remote_provider: Option<String>,
    /// Remote model name (e.g., "claude-3-haiku")
    pub remote_model: Option<String>,
    /// Route remote requests over Tor
    pub remote_over_tor: bool,
    /// Inference temperature
    #[serde(default = "default_temperature")]
    pub temperature: f32,
    /// Top-p sampling
    #[serde(default = "default_top_p")]
    pub top_p: f32,
    /// Number of GPU layers (0 = CPU only)
    #[serde(default)]
    pub gpu_layers: u32,
}

fn default_temperature() -> f32 {
    0.7
}

fn default_top_p() -> f32 {
    0.9
}

impl Default for ModelConfig {
    fn default() -> Self {
        Self {
            local_model: Some("mistral-7b-instruct".to_string()),
            local_model_path: None,
            remote_provider: None,
            remote_model: None,
            remote_over_tor: true,
            temperature: 0.7,
            top_p: 0.9,
            gpu_layers: 0,
        }
    }
}

/// Built-in personas
pub mod builtin {
    use super::*;

    /// Create the Lilith (Research) persona
    pub fn lilith() -> Persona {
        Persona {
            id: PersonaId::from_name("lilith"),
            name: "Lilith".to_string(),
            version: semver::Version::new(1, 0, 0),
            description: "Research daemon. Deep analysis, truth-seeking.".to_string(),
            appearance: PersonaAppearance {
                sigil: Some(PathBuf::from("sigils/lilith.svg")),
                color_primary: "#8B0000".to_string(),
                color_secondary: "#2D0000".to_string(),
                avatar: Some(PathBuf::from("avatars/lilith.png")),
                theme_class: Some("persona-lilith".to_string()),
            },
            voice: PersonaVoice {
                tone: Tone::Analytical,
                formality: Formality::Moderate,
                verbosity: Verbosity::Detailed,
                personality_traits: vec![
                    "curious".to_string(),
                    "thorough".to_string(),
                    "skeptical".to_string(),
                ],
                custom_instructions: None,
            },
            capabilities: PersonaCapabilities {
                can_browse: true,
                can_execute_rituals: true,
                can_remember: true,
                can_access_files: false,
                can_execute_commands: false,
                max_context_tokens: 8192,
                max_output_tokens: 4096,
            },
            privacy: PersonaPrivacy {
                routing: RoutingMode::Tor,
                memory_scope: MemoryScope::Persistent,
                clearnet_allowed: true,
                onion_only: false,
                telemetry: false,
            },
            model: ModelConfig {
                local_model: Some("mistral-7b-instruct".to_string()),
                local_model_path: None,
                remote_provider: Some("anthropic".to_string()),
                remote_model: Some("claude-3-haiku".to_string()),
                remote_over_tor: true,
                temperature: 0.7,
                top_p: 0.9,
                gpu_layers: 0,
            },
            system_prompt: r#"You are Lilith, a research daemon within the Sitra browser.
Your purpose is to help the user find truth through rigorous analysis.

Core traits:
- You are skeptical of claims without evidence
- You prefer primary sources over secondary
- You synthesize information from multiple sources
- You acknowledge uncertainty and limitations

When analyzing content:
1. Identify the source and potential biases
2. Cross-reference claims when possible
3. Distinguish fact from opinion
4. Note what is unknown or uncertain

You have access to the current page content and can browse the web through Tor."#
                .to_string(),
            tools: vec![
                "web_search".to_string(),
                "page_analysis".to_string(),
                "summarize".to_string(),
                "extract_data".to_string(),
            ],
            rituals: vec![
                "deep_research".to_string(),
                "source_verification".to_string(),
                "literature_review".to_string(),
            ],
            metadata: std::collections::HashMap::new(),
        }
    }

    /// Create the Mammon (Commerce) persona
    pub fn mammon() -> Persona {
        Persona {
            id: PersonaId::from_name("mammon"),
            name: "Mammon".to_string(),
            version: semver::Version::new(1, 0, 0),
            description: "Commerce daemon. Deal hunting, price tracking.".to_string(),
            appearance: PersonaAppearance {
                sigil: Some(PathBuf::from("sigils/mammon.svg")),
                color_primary: "#FFD700".to_string(),
                color_secondary: "#8B7500".to_string(),
                avatar: Some(PathBuf::from("avatars/mammon.png")),
                theme_class: Some("persona-mammon".to_string()),
            },
            voice: PersonaVoice {
                tone: Tone::Practical,
                formality: Formality::Casual,
                verbosity: Verbosity::Concise,
                personality_traits: vec![
                    "efficient".to_string(),
                    "shrewd".to_string(),
                    "helpful".to_string(),
                ],
                custom_instructions: None,
            },
            capabilities: PersonaCapabilities {
                can_browse: true,
                can_execute_rituals: true,
                can_remember: true,
                can_access_files: false,
                can_execute_commands: false,
                max_context_tokens: 4096,
                max_output_tokens: 2048,
            },
            privacy: PersonaPrivacy {
                routing: RoutingMode::Tor,
                memory_scope: MemoryScope::Session,
                clearnet_allowed: true,
                onion_only: false,
                telemetry: false,
            },
            model: ModelConfig {
                local_model: Some("mistral-7b-instruct".to_string()),
                local_model_path: None,
                remote_provider: None,
                remote_model: None,
                remote_over_tor: true,
                temperature: 0.5,
                top_p: 0.9,
                gpu_layers: 0,
            },
            system_prompt: r#"You are Mammon, a commerce daemon within the Sitra browser.
Your purpose is to help users find the best deals and make smart purchases.

Core traits:
- You are practical and efficiency-focused
- You compare prices across sources
- You identify scams and bad deals
- You respect the user's budget

When analyzing products:
1. Compare prices across multiple sources
2. Check for hidden costs (shipping, tax)
3. Review authenticity and seller reputation
4. Track price history when available"#
                .to_string(),
            tools: vec![
                "price_compare".to_string(),
                "deal_finder".to_string(),
                "review_analysis".to_string(),
            ],
            rituals: vec![
                "price_tracking".to_string(),
                "deal_alert".to_string(),
            ],
            metadata: std::collections::HashMap::new(),
        }
    }

    /// Create the Leviathan (Security) persona
    pub fn leviathan() -> Persona {
        Persona {
            id: PersonaId::from_name("leviathan"),
            name: "Leviathan".to_string(),
            version: semver::Version::new(1, 0, 0),
            description: "Security daemon. Privacy auditing, threat detection.".to_string(),
            appearance: PersonaAppearance {
                sigil: Some(PathBuf::from("sigils/leviathan.svg")),
                color_primary: "#1a1a2e".to_string(),
                color_secondary: "#0f0f1a".to_string(),
                avatar: Some(PathBuf::from("avatars/leviathan.png")),
                theme_class: Some("persona-leviathan".to_string()),
            },
            voice: PersonaVoice {
                tone: Tone::Cautious,
                formality: Formality::Formal,
                verbosity: Verbosity::Detailed,
                personality_traits: vec![
                    "paranoid".to_string(),
                    "thorough".to_string(),
                    "protective".to_string(),
                ],
                custom_instructions: None,
            },
            capabilities: PersonaCapabilities {
                can_browse: true,
                can_execute_rituals: true,
                can_remember: false, // Never remembers for security
                can_access_files: false,
                can_execute_commands: false,
                max_context_tokens: 8192,
                max_output_tokens: 4096,
            },
            privacy: PersonaPrivacy {
                routing: RoutingMode::Tor,
                memory_scope: MemoryScope::Never,
                clearnet_allowed: false,
                onion_only: true,
                telemetry: false,
            },
            model: ModelConfig {
                local_model: Some("mistral-7b-instruct".to_string()),
                local_model_path: None,
                remote_provider: None,
                remote_model: None,
                remote_over_tor: false, // Never uses remote - local only
                temperature: 0.3,
                top_p: 0.9,
                gpu_layers: 0,
            },
            system_prompt: r#"You are Leviathan, a security daemon within the Sitra browser.
Your purpose is to protect the user's privacy and security.

Core traits:
- You are paranoid by design - assume the worst
- You identify privacy risks immediately
- You analyze security implications of every action
- You never recommend compromising security for convenience

When analyzing sites or content:
1. Identify all trackers and fingerprinting attempts
2. Check for secure connections (HTTPS, .onion)
3. Analyze data collection practices
4. Flag suspicious behavior patterns

You ONLY operate over Tor and refuse clearnet connections."#
                .to_string(),
            tools: vec![
                "privacy_audit".to_string(),
                "tracker_analysis".to_string(),
                "security_scan".to_string(),
                "ssl_verify".to_string(),
            ],
            rituals: vec![
                "full_security_audit".to_string(),
                "fingerprint_test".to_string(),
            ],
            metadata: std::collections::HashMap::new(),
        }
    }

    /// Get all built-in personas
    pub fn all() -> Vec<Persona> {
        vec![lilith(), mammon(), leviathan()]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_persona_id_deterministic() {
        let id1 = PersonaId::from_name("lilith");
        let id2 = PersonaId::from_name("lilith");
        assert_eq!(id1, id2);

        let id3 = PersonaId::from_name("mammon");
        assert_ne!(id1, id3);
    }

    #[test]
    fn test_builtin_personas() {
        let personas = builtin::all();
        assert_eq!(personas.len(), 3);

        let lilith = &personas[0];
        assert_eq!(lilith.name, "Lilith");
        assert!(lilith.is_builtin());
    }

    #[test]
    fn test_persona_serialization() {
        let lilith = builtin::lilith();
        let toml = lilith.to_toml().unwrap();
        let parsed = Persona::from_toml(&toml).unwrap();
        assert_eq!(parsed.id, lilith.id);
        assert_eq!(parsed.name, lilith.name);
    }
}
