//! PAM authentication backend

use crate::auth::{AccountStatus, AuthChallenge, AuthInfo, AuthResult, Authenticator, ChallengeType, Credentials};
use anyhow::{Result, anyhow};
use async_trait::async_trait;
use std::collections::HashMap;
use std::ffi::{CStr, CString};
use std::sync::RwLock;
use tracing::{info, warn, debug};

/// PAM-based authenticator
pub struct PamAuthenticator {
    service: String,
    conversations: RwLock<HashMap<String, PamConversation>>,
}

struct PamConversation {
    username: String,
    started_at: std::time::Instant,
}

impl PamAuthenticator {
    pub fn new(service: String) -> Self {
        Self {
            service,
            conversations: RwLock::new(HashMap::new()),
        }
    }

    /// Authenticate using PAM
    fn pam_authenticate(&self, username: &str, password: &str) -> Result<AuthInfo> {
        debug!("PAM authentication for user: {}", username);

        // In a real implementation, this would use the pam crate
        // to perform actual PAM authentication.
        //
        // Example with pam crate:
        // let mut auth = pam::Authenticator::with_password(&self.service)?;
        // auth.get_handler().set_credentials(username, password);
        // auth.authenticate()?;
        // auth.open_session()?;

        // For now, simulate PAM by checking /etc/shadow (simplified)
        // In production, always use proper PAM

        // Get user info
        let user_info = crate::user::get_user_info(username)?;

        // Simulate successful auth for development
        // NEVER do this in production!
        #[cfg(debug_assertions)]
        {
            if password == "debug" {
                warn!("DEBUG: Allowing debug password for {}", username);
                return Ok(AuthInfo {
                    username: username.to_string(),
                    uid: user_info.uid,
                    gid: user_info.gid,
                    home: user_info.home,
                    shell: user_info.shell,
                    groups: user_info.groups,
                });
            }
        }

        // In production, use PAM
        self.do_pam_auth(username, password, &user_info)
    }

    fn do_pam_auth(&self, username: &str, _password: &str, user_info: &crate::user::UserInfo) -> Result<AuthInfo> {
        // This would be the actual PAM implementation
        // For now, return an error as we can't actually authenticate
        // without proper PAM setup

        // Placeholder: In real implementation, use pam crate
        /*
        use pam::Client;

        let mut client = Client::with_password(&self.service)?;
        client.conversation_mut()
            .set_credentials(username, password);

        match client.authenticate() {
            Ok(()) => {
                client.open_session()?;
                Ok(AuthInfo { ... })
            }
            Err(e) => Err(anyhow!("PAM auth failed: {}", e))
        }
        */

        // For development without PAM:
        Err(anyhow!(
            "PAM authentication not available in this build. \
             Use a proper PAM-enabled build for authentication."
        ))
    }

    fn check_pam_account(&self, username: &str) -> Result<AccountStatus> {
        // Would use pam_acct_mgmt() to check account status
        // For now, assume valid if user exists

        match crate::user::get_user_info(username) {
            Ok(_) => Ok(AccountStatus::Valid),
            Err(_) => Ok(AccountStatus::NotFound),
        }
    }
}

#[async_trait]
impl Authenticator for PamAuthenticator {
    async fn authenticate(&self, credentials: &Credentials) -> Result<AuthResult> {
        let password = credentials.password_str()
            .ok_or_else(|| anyhow!("Invalid password encoding"))?;

        match self.pam_authenticate(&credentials.username, password) {
            Ok(info) => {
                info!("Authentication successful for {}", credentials.username);
                Ok(AuthResult::Success(info))
            }
            Err(e) => {
                warn!("Authentication failed for {}: {}", credentials.username, e);
                Ok(AuthResult::Failure(e.to_string()))
            }
        }
    }

    async fn validate_session(&self, _token: &str) -> Result<bool> {
        // Would validate session token
        Ok(false)
    }

    async fn start_auth(&self, username: &str) -> Result<AuthChallenge> {
        let mut convs = self.conversations.write()
            .map_err(|_| anyhow!("Lock poisoned"))?;

        convs.insert(username.to_string(), PamConversation {
            username: username.to_string(),
            started_at: std::time::Instant::now(),
        });

        Ok(AuthChallenge {
            challenge_type: ChallengeType::Password,
            message: "Password: ".to_string(),
        })
    }

    async fn respond(&self, username: &str, response: &str) -> Result<AuthResult> {
        let credentials = Credentials::new(username, response);
        self.authenticate(&credentials).await
    }

    async fn close(&self, username: &str) -> Result<()> {
        let mut convs = self.conversations.write()
            .map_err(|_| anyhow!("Lock poisoned"))?;

        convs.remove(username);
        Ok(())
    }

    async fn check_account(&self, username: &str) -> Result<AccountStatus> {
        self.check_pam_account(username)
    }
}

/// Environment variables to set for PAM sessions
pub fn pam_environment(username: &str, uid: u32) -> HashMap<String, String> {
    let mut env = HashMap::new();

    env.insert("USER".to_string(), username.to_string());
    env.insert("LOGNAME".to_string(), username.to_string());
    env.insert("HOME".to_string(), format!("/home/{}", username));
    env.insert("SHELL".to_string(), "/bin/bash".to_string());
    env.insert("PATH".to_string(), "/usr/local/bin:/usr/bin:/bin".to_string());

    // XDG directories
    env.insert("XDG_RUNTIME_DIR".to_string(), format!("/run/user/{}", uid));
    env.insert("XDG_SESSION_TYPE".to_string(), "tty".to_string());
    env.insert("XDG_SESSION_CLASS".to_string(), "user".to_string());

    env
}

/// Set up XDG runtime directory for user
pub fn setup_xdg_runtime(uid: u32, gid: u32) -> Result<String> {
    let path = format!("/run/user/{}", uid);

    // Create directory if it doesn't exist
    if !std::path::Path::new(&path).exists() {
        std::fs::create_dir_all(&path)?;
    }

    // Set ownership and permissions
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700))?;

        unsafe {
            let path_c = CString::new(path.as_str())?;
            libc::chown(path_c.as_ptr(), uid, gid);
        }
    }

    Ok(path)
}
