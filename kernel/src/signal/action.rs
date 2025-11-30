//! Signal action (handler) configuration

use super::SigSet;
use alloc::boxed::Box;

/// Signal action configuration
#[derive(Clone, Debug)]
pub struct SigAction {
    /// Signal handler
    pub handler: SigHandler,
    /// Additional signals to block during handler
    pub mask: SigSet,
    /// Action flags
    pub flags: SigActionFlags,
    /// Restorer function (for sigreturn)
    pub restorer: Option<u64>,
}

impl Default for SigAction {
    fn default() -> Self {
        Self {
            handler: SigHandler::Default,
            mask: SigSet::empty(),
            flags: SigActionFlags::empty(),
            restorer: None,
        }
    }
}

impl SigAction {
    /// Create new action with default handler
    pub fn new_default() -> Self {
        Self::default()
    }

    /// Create new action that ignores the signal
    pub fn new_ignore() -> Self {
        Self {
            handler: SigHandler::Ignore,
            ..Default::default()
        }
    }

    /// Create new action with custom handler
    pub fn new_handler(handler_addr: u64) -> Self {
        Self {
            handler: SigHandler::Handler(handler_addr),
            ..Default::default()
        }
    }

    /// Create new action with sigaction-style handler
    pub fn new_sigaction(handler_addr: u64) -> Self {
        Self {
            handler: SigHandler::SigAction(handler_addr),
            flags: SigActionFlags::SIGINFO,
            ..Default::default()
        }
    }

    /// Builder: set mask
    pub fn with_mask(mut self, mask: SigSet) -> Self {
        self.mask = mask;
        self
    }

    /// Builder: set flags
    pub fn with_flags(mut self, flags: SigActionFlags) -> Self {
        self.flags = flags;
        self
    }

    /// Builder: set restorer
    pub fn with_restorer(mut self, restorer: u64) -> Self {
        self.restorer = Some(restorer);
        self
    }
}

/// Signal handler type
#[derive(Clone, Debug)]
pub enum SigHandler {
    /// Default action (SIG_DFL)
    Default,
    /// Ignore signal (SIG_IGN)
    Ignore,
    /// Simple handler: void handler(int signum)
    Handler(u64),
    /// Sigaction handler: void handler(int signum, siginfo_t *info, void *context)
    SigAction(u64),
}

impl SigHandler {
    /// Check if this is a custom handler
    pub fn is_custom(&self) -> bool {
        matches!(self, SigHandler::Handler(_) | SigHandler::SigAction(_))
    }

    /// Get handler address if custom
    pub fn address(&self) -> Option<u64> {
        match self {
            SigHandler::Handler(addr) | SigHandler::SigAction(addr) => Some(*addr),
            _ => None,
        }
    }
}

bitflags::bitflags! {
    /// Signal action flags
    #[derive(Clone, Copy, Debug, Default)]
    pub struct SigActionFlags: u32 {
        /// Don't add signal to mask during handler
        const NODEFER = 1 << 0;
        /// Don't create zombie children
        const NOCLDSTOP = 1 << 1;
        /// Don't notify parent when child stops
        const NOCLDWAIT = 1 << 2;
        /// Handler receives siginfo_t
        const SIGINFO = 1 << 3;
        /// Use alternate signal stack
        const ONSTACK = 1 << 4;
        /// Reset handler to default after delivery
        const RESETHAND = 1 << 5;
        /// Restart interrupted syscalls
        const RESTART = 1 << 6;
        /// Expose sa_restorer field
        const RESTORER = 1 << 7;
    }
}

/// Signal disposition (computed from action)
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SignalDisposition {
    /// Use default action
    Default,
    /// Ignore the signal
    Ignore,
    /// Call user-space handler
    Handle,
}

impl From<&SigAction> for SignalDisposition {
    fn from(action: &SigAction) -> Self {
        match action.handler {
            SigHandler::Default => SignalDisposition::Default,
            SigHandler::Ignore => SignalDisposition::Ignore,
            SigHandler::Handler(_) | SigHandler::SigAction(_) => SignalDisposition::Handle,
        }
    }
}
