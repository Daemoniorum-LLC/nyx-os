//! Security integration
//!
//! Guardian-mediated security for display operations.

use crate::config::SecurityConfig;
use anyhow::Result;
use libnyx_ipc::guardian::GuardianClient;
use libnyx_ipc::protocol::{CapabilityRequest, Decision};
use std::collections::HashMap;
use std::path::PathBuf;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

/// Security manager
pub struct SecurityManager {
    /// Configuration
    config: SecurityConfig,
    /// Guardian client
    guardian: RwLock<Option<GuardianClient>>,
    /// Guardian socket path
    guardian_socket: PathBuf,
    /// Client capabilities cache
    client_caps: RwLock<HashMap<u32, Vec<String>>>,
}

impl SecurityManager {
    /// Create new security manager
    pub fn new(config: &SecurityConfig, guardian_socket: PathBuf) -> Result<Self> {
        Ok(Self {
            config: config.clone(),
            guardian: RwLock::new(None),
            guardian_socket,
            client_caps: RwLock::new(HashMap::new()),
        })
    }

    /// Initialize Guardian connection
    pub async fn connect_guardian(&self) -> Result<()> {
        if !self.config.guardian_enabled {
            return Ok(());
        }

        let client = GuardianClient::with_socket(&self.guardian_socket);
        *self.guardian.write().await = Some(client);
        info!("Connected to Guardian");
        Ok(())
    }

    /// Check if a client can perform screen capture
    pub async fn can_capture(&self, client_id: u32, client_path: &str) -> bool {
        if !self.config.capture_requires_cap {
            return true;
        }

        self.check_capability(client_id, client_path, "display:capture", None).await
    }

    /// Check if a client can grab input
    pub async fn can_grab_input(&self, client_id: u32, client_path: &str) -> bool {
        if !self.config.input_grab_requires_cap {
            return true;
        }

        self.check_capability(client_id, client_path, "input:grab", None).await
    }

    /// Check if a client can use privileged protocols
    pub async fn can_use_protocol(&self, client_id: u32, client_path: &str, protocol: &str) -> bool {
        // Always allow standard protocols
        if !self.is_privileged_protocol(protocol) {
            return true;
        }

        self.check_capability(client_id, client_path, &format!("wayland:{}", protocol), None).await
    }

    /// Check if a protocol is privileged
    fn is_privileged_protocol(&self, protocol: &str) -> bool {
        let privileged = [
            "zwlr_screencopy_manager",
            "zwlr_export_dmabuf_manager",
            "zwlr_input_inhibitor_manager",
            "zwp_input_method_manager",
            "zwp_virtual_keyboard_manager",
            "zwlr_virtual_pointer_manager",
            "ext_session_lock_manager",
        ];

        privileged.contains(&protocol) ||
        self.config.privileged_protocols.iter().any(|p| p == protocol)
    }

    /// Check capability with Guardian
    async fn check_capability(
        &self,
        client_id: u32,
        client_path: &str,
        capability: &str,
        resource: Option<&str>,
    ) -> bool {
        if !self.config.guardian_enabled {
            return true;
        }

        // Check cache first
        {
            let cache = self.client_caps.read().await;
            if let Some(caps) = cache.get(&client_id) {
                if caps.iter().any(|c| c == capability || c == "*") {
                    return true;
                }
            }
        }

        // Check with Guardian
        let mut guardian = self.guardian.write().await;

        if guardian.is_none() {
            // Try to connect
            *guardian = Some(GuardianClient::with_socket(&self.guardian_socket));
        }

        if let Some(ref mut client) = *guardian {
            let mut request = CapabilityRequest::new(capability);
            request.process_path = client_path.to_string();
            request.pid = client_id;
            if let Some(res) = resource {
                request = request.with_resource(res);
            }

            match client.check_capability_full(request).await {
                Ok(decision) => {
                    let allowed = matches!(decision.decision, Decision::Allow | Decision::Sandbox);

                    // Cache if allowed
                    if allowed {
                        let mut cache = self.client_caps.write().await;
                        cache.entry(client_id)
                            .or_insert_with(Vec::new)
                            .push(capability.to_string());
                    }

                    debug!(
                        "Capability check: client={}, cap={}, result={}",
                        client_id, capability, allowed
                    );
                    allowed
                }
                Err(e) => {
                    warn!("Guardian check failed: {} - allowing by default", e);
                    true
                }
            }
        } else {
            true // Allow if Guardian unavailable
        }
    }

    /// Client disconnected - clear cache
    pub async fn client_disconnected(&self, client_id: u32) {
        let mut cache = self.client_caps.write().await;
        cache.remove(&client_id);
    }

    /// Grant a capability to a client
    pub async fn grant_capability(&self, client_id: u32, capability: &str) {
        let mut cache = self.client_caps.write().await;
        cache.entry(client_id)
            .or_insert_with(Vec::new)
            .push(capability.to_string());
    }

    /// Revoke a capability from a client
    pub async fn revoke_capability(&self, client_id: u32, capability: &str) {
        let mut cache = self.client_caps.write().await;
        if let Some(caps) = cache.get_mut(&client_id) {
            caps.retain(|c| c != capability);
        }
    }
}

/// Display capability types
pub mod capabilities {
    pub const CAPTURE: &str = "display:capture";
    pub const INPUT_GRAB: &str = "input:grab";
    pub const FULLSCREEN: &str = "display:fullscreen";
    pub const LAYER_SHELL: &str = "display:layer_shell";
    pub const SESSION_LOCK: &str = "display:session_lock";
}
