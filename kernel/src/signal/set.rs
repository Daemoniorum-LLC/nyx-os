//! Signal set operations
//!
//! Provides a bitmap for representing sets of signals.

use super::Signal;

/// Signal set (bitmap of 64 signals)
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SigSet(u64);

impl SigSet {
    /// Empty signal set
    pub const fn empty() -> Self {
        Self(0)
    }

    /// Full signal set (all signals)
    pub const fn full() -> Self {
        Self(!0)
    }

    /// Create a set with a single signal
    pub fn single(signal: Signal) -> Self {
        Self(1u64 << signal.as_raw())
    }

    /// Create from raw bitmap
    pub const fn from_raw(bits: u64) -> Self {
        Self(bits)
    }

    /// Get raw bitmap
    pub const fn as_raw(&self) -> u64 {
        self.0
    }

    /// Check if signal is in set
    pub fn contains(&self, signum: u8) -> bool {
        if signum == 0 || signum > 64 {
            return false;
        }
        (self.0 & (1u64 << signum)) != 0
    }

    /// Check if Signal is in set
    pub fn contains_signal(&self, signal: Signal) -> bool {
        self.contains(signal.as_raw())
    }

    /// Add signal to set
    pub fn add(&mut self, signum: u8) {
        if signum > 0 && signum <= 64 {
            self.0 |= 1u64 << signum;
        }
    }

    /// Add Signal to set
    pub fn add_signal(&mut self, signal: Signal) {
        self.add(signal.as_raw());
    }

    /// Remove signal from set
    pub fn remove(&mut self, signum: u8) {
        if signum > 0 && signum <= 64 {
            self.0 &= !(1u64 << signum);
        }
    }

    /// Remove Signal from set
    pub fn remove_signal(&mut self, signal: Signal) {
        self.remove(signal.as_raw());
    }

    /// Check if set is empty
    pub fn is_empty(&self) -> bool {
        self.0 == 0
    }

    /// Count signals in set
    pub fn count(&self) -> u32 {
        self.0.count_ones()
    }

    /// Union with another set
    pub fn union(&self, other: &SigSet) -> SigSet {
        SigSet(self.0 | other.0)
    }

    /// Intersection with another set
    pub fn intersection(&self, other: &SigSet) -> SigSet {
        SigSet(self.0 & other.0)
    }

    /// Difference with another set (self - other)
    pub fn difference(&self, other: &SigSet) -> SigSet {
        SigSet(self.0 & !other.0)
    }

    /// Complement of set
    pub fn complement(&self) -> SigSet {
        SigSet(!self.0)
    }

    /// Get the lowest signal number in the set
    pub fn first(&self) -> Option<u8> {
        if self.0 == 0 {
            None
        } else {
            Some(self.0.trailing_zeros() as u8)
        }
    }

    /// Iterate over signals in set
    pub fn iter(&self) -> SigSetIter {
        SigSetIter {
            remaining: self.0,
        }
    }

    /// Fill set with all signals except SIGKILL and SIGSTOP
    pub fn fill_catchable(&mut self) {
        self.0 = !0;
        self.remove(Signal::SIGKILL.as_raw());
        self.remove(Signal::SIGSTOP.as_raw());
    }
}

/// Iterator over signals in a set
pub struct SigSetIter {
    remaining: u64,
}

impl Iterator for SigSetIter {
    type Item = u8;

    fn next(&mut self) -> Option<Self::Item> {
        if self.remaining == 0 {
            None
        } else {
            let sig = self.remaining.trailing_zeros() as u8;
            self.remaining &= !(1u64 << sig);
            Some(sig)
        }
    }
}

impl core::ops::BitOr for SigSet {
    type Output = SigSet;

    fn bitor(self, rhs: Self) -> Self::Output {
        SigSet(self.0 | rhs.0)
    }
}

impl core::ops::BitOrAssign for SigSet {
    fn bitor_assign(&mut self, rhs: Self) {
        self.0 |= rhs.0;
    }
}

impl core::ops::BitAnd for SigSet {
    type Output = SigSet;

    fn bitand(self, rhs: Self) -> Self::Output {
        SigSet(self.0 & rhs.0)
    }
}

impl core::ops::BitAndAssign for SigSet {
    fn bitand_assign(&mut self, rhs: Self) {
        self.0 &= rhs.0;
    }
}

impl core::ops::Not for SigSet {
    type Output = SigSet;

    fn not(self) -> Self::Output {
        SigSet(!self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sigset_empty() {
        let set = SigSet::empty();
        assert!(set.is_empty());
        assert!(!set.contains(1));
    }

    #[test]
    fn test_sigset_add_remove() {
        let mut set = SigSet::empty();
        set.add(Signal::SIGINT.as_raw());
        assert!(set.contains(Signal::SIGINT.as_raw()));
        set.remove(Signal::SIGINT.as_raw());
        assert!(!set.contains(Signal::SIGINT.as_raw()));
    }

    #[test]
    fn test_sigset_union() {
        let mut a = SigSet::empty();
        a.add(1);
        a.add(2);

        let mut b = SigSet::empty();
        b.add(2);
        b.add(3);

        let c = a.union(&b);
        assert!(c.contains(1));
        assert!(c.contains(2));
        assert!(c.contains(3));
    }
}
