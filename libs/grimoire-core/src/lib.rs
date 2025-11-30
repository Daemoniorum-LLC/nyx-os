//! # Grimoire Core
//!
//! Shared types and traits for the Grimoire system.
//!
//! This library provides the common foundation for:
//! - DaemonOS Grimoire agent (system-level persona + config management)
//! - Sitra browser (persona-powered browsing)
//!
//! ## Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────┐
//! │                    SITRA BROWSER                         │
//! │  Uses: GrimoireClient to communicate with daemon        │
//! └──────────────────────────┬──────────────────────────────┘
//!                            │ IPC (Unix Socket)
//! ┌──────────────────────────┴──────────────────────────────┐
//! │              DAEMONOS GRIMOIRE DAEMON                    │
//! │  ┌─────────────────┐  ┌─────────────────┐              │
//! │  │  PersonaStore   │  │  SettingsStore  │              │
//! │  │  (personas/)    │  │  (config/)      │              │
//! │  └────────┬────────┘  └────────┬────────┘              │
//! │           └────────────────────┘                        │
//! │                        │                                │
//! │              ┌─────────┴─────────┐                      │
//! │              │   MemoryManager   │                      │
//! │              │ (Cipher-encrypted)│                      │
//! │              └───────────────────┘                      │
//! └─────────────────────────────────────────────────────────┘
//! ```

mod persona;
mod memory;
mod ritual;
mod ipc;
mod error;

pub use persona::*;
pub use memory::*;
pub use ritual::*;
pub use ipc::*;
pub use error::*;

/// Re-export common types
pub mod prelude {
    pub use crate::persona::{
        Persona, PersonaId, PersonaAppearance, PersonaVoice,
        PersonaPrivacy, PersonaCapabilities, ModelConfig,
        RoutingMode, MemoryScope, Tone, Formality, Verbosity,
    };
    pub use crate::memory::{
        PersonaMemory, MemoryEntry, MemoryEntryType, MemoryConfig,
    };
    pub use crate::ritual::{
        Ritual, RitualStep, RitualTrigger, RitualId,
    };
    pub use crate::ipc::{
        GrimoireRequest, GrimoireResponse, PersonaEvent,
    };
    pub use crate::error::GrimoireError;
}
