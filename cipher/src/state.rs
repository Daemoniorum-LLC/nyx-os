//! Cipher daemon state

use crate::keyring::Keyring;
use crate::session::SessionManager;

/// Daemon state
pub struct CipherState {
    pub keyring: Keyring,
    pub sessions: SessionManager,
    pub data_dir: String,
}
