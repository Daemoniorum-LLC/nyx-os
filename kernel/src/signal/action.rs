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
    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sigaction_default() {
        let action = SigAction::default();
        assert!(matches!(action.handler, SigHandler::Default));
        assert!(action.mask.is_empty());
        assert!(action.flags.is_empty());
        assert!(action.restorer.is_none());
    }

    #[test]
    fn test_sigaction_new_default() {
        let action = SigAction::new_default();
        assert!(matches!(action.handler, SigHandler::Default));
    }

    #[test]
    fn test_sigaction_new_ignore() {
        let action = SigAction::new_ignore();
        assert!(matches!(action.handler, SigHandler::Ignore));
    }

    #[test]
    fn test_sigaction_new_handler() {
        let addr = 0x1000u64;
        let action = SigAction::new_handler(addr);
        assert!(matches!(action.handler, SigHandler::Handler(a) if a == addr));
    }

    #[test]
    fn test_sigaction_new_sigaction() {
        let addr = 0x2000u64;
        let action = SigAction::new_sigaction(addr);
        assert!(matches!(action.handler, SigHandler::SigAction(a) if a == addr));
        assert!(action.flags.contains(SigActionFlags::SIGINFO));
    }

    #[test]
    fn test_sigaction_with_mask() {
        let mut mask = SigSet::empty();
        mask.add(2); // SIGINT
        mask.add(15); // SIGTERM

        let action = SigAction::new_handler(0x1000).with_mask(mask.clone());
        assert_eq!(action.mask, mask);
    }

    #[test]
    fn test_sigaction_with_flags() {
        let flags = SigActionFlags::RESTART | SigActionFlags::NODEFER;
        let action = SigAction::new_handler(0x1000).with_flags(flags);
        assert_eq!(action.flags, flags);
    }

    #[test]
    fn test_sigaction_with_restorer() {
        let restorer = 0x3000u64;
        let action = SigAction::new_handler(0x1000).with_restorer(restorer);
        assert_eq!(action.restorer, Some(restorer));
    }

    #[test]
    fn test_sigaction_builder_chain() {
        let mut mask = SigSet::empty();
        mask.add(2);
        let flags = SigActionFlags::RESTART;
        let restorer = 0x3000u64;

        let action = SigAction::new_handler(0x1000)
            .with_mask(mask.clone())
            .with_flags(flags)
            .with_restorer(restorer);

        assert!(matches!(action.handler, SigHandler::Handler(0x1000)));
        assert_eq!(action.mask, mask);
        assert_eq!(action.flags, flags);
        assert_eq!(action.restorer, Some(restorer));
    }

    #[test]
    fn test_sighandler_is_custom() {
        assert!(!SigHandler::Default.is_custom());
        assert!(!SigHandler::Ignore.is_custom());
        assert!(SigHandler::Handler(0x1000).is_custom());
        assert!(SigHandler::SigAction(0x2000).is_custom());
    }

    #[test]
    fn test_sighandler_address() {
        assert_eq!(SigHandler::Default.address(), None);
        assert_eq!(SigHandler::Ignore.address(), None);
        assert_eq!(SigHandler::Handler(0x1000).address(), Some(0x1000));
        assert_eq!(SigHandler::SigAction(0x2000).address(), Some(0x2000));
    }

    #[test]
    fn test_sigactionflags_empty() {
        let flags = SigActionFlags::empty();
        assert!(flags.is_empty());
        assert!(!flags.contains(SigActionFlags::RESTART));
    }

    #[test]
    fn test_sigactionflags_individual() {
        assert!(!SigActionFlags::NODEFER.is_empty());
        assert!(!SigActionFlags::NOCLDSTOP.is_empty());
        assert!(!SigActionFlags::NOCLDWAIT.is_empty());
        assert!(!SigActionFlags::SIGINFO.is_empty());
        assert!(!SigActionFlags::ONSTACK.is_empty());
        assert!(!SigActionFlags::RESETHAND.is_empty());
        assert!(!SigActionFlags::RESTART.is_empty());
        assert!(!SigActionFlags::RESTORER.is_empty());
    }

    #[test]
    fn test_sigactionflags_combine() {
        let flags = SigActionFlags::RESTART | SigActionFlags::NODEFER | SigActionFlags::SIGINFO;
        assert!(flags.contains(SigActionFlags::RESTART));
        assert!(flags.contains(SigActionFlags::NODEFER));
        assert!(flags.contains(SigActionFlags::SIGINFO));
        assert!(!flags.contains(SigActionFlags::ONSTACK));
    }

    #[test]
    fn test_signal_disposition_from_default() {
        let action = SigAction::new_default();
        let disp = SignalDisposition::from(&action);
        assert_eq!(disp, SignalDisposition::Default);
    }

    #[test]
    fn test_signal_disposition_from_ignore() {
        let action = SigAction::new_ignore();
        let disp = SignalDisposition::from(&action);
        assert_eq!(disp, SignalDisposition::Ignore);
    }

    #[test]
    fn test_signal_disposition_from_handler() {
        let action = SigAction::new_handler(0x1000);
        let disp = SignalDisposition::from(&action);
        assert_eq!(disp, SignalDisposition::Handle);
    }

    #[test]
    fn test_signal_disposition_from_sigaction() {
        let action = SigAction::new_sigaction(0x2000);
        let disp = SignalDisposition::from(&action);
        assert_eq!(disp, SignalDisposition::Handle);
    }
}
