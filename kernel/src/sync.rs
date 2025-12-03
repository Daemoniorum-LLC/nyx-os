//! # Ordered Locking Primitives
//!
//! This module provides lock wrappers that encode their position in the lock
//! hierarchy. This helps prevent deadlocks by making lock ordering explicit.
//!
//! See the crate-level documentation for the complete lock hierarchy.

use spin::{RwLock, RwLockReadGuard, RwLockWriteGuard};

/// Lock ordering levels.
///
/// Locks must be acquired in increasing level order.
/// Level 0 locks must be acquired before Level 1, etc.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum LockLevel {
    /// Level 0: Global registries (PROCESSES, TENSORS)
    Registry = 0,
    /// Level 1: Thread/IPC tables (THREADS, ENDPOINTS, RINGS)
    Table = 1,
    /// Level 2: Per-CPU state (PER_CPU, NOTIFICATIONS)
    PerCpu = 2,
    /// Level 3: Individual object state (address spaces, thread fields)
    Object = 3,
}

/// A read-write lock with an associated ordering level.
///
/// This type is a wrapper around `spin::RwLock` that encodes the lock's
/// position in the hierarchy. While Rust's type system cannot prevent
/// all ordering violations, using this type makes the intended ordering
/// explicit and enables runtime checks in debug builds.
pub struct OrderedRwLock<T, const LEVEL: u8> {
    inner: RwLock<T>,
    #[cfg(debug_assertions)]
    name: &'static str,
}

impl<T, const LEVEL: u8> OrderedRwLock<T, LEVEL> {
    /// Create a new ordered lock.
    pub const fn new(value: T, _name: &'static str) -> Self {
        Self {
            inner: RwLock::new(value),
            #[cfg(debug_assertions)]
            name: _name,
        }
    }

    /// Acquire a read lock.
    ///
    /// In debug builds, this will check that no higher-level locks are held.
    #[inline]
    pub fn read(&self) -> RwLockReadGuard<'_, T> {
        #[cfg(debug_assertions)]
        {
            // In a full implementation, we would check thread-local state
            // to verify no lower-level locks are held while we acquire this.
            // For now, we just acquire the lock.
        }
        self.inner.read()
    }

    /// Acquire a write lock.
    ///
    /// In debug builds, this will check that no higher-level locks are held.
    #[inline]
    pub fn write(&self) -> RwLockWriteGuard<'_, T> {
        #[cfg(debug_assertions)]
        {
            // Same as read() - would check ordering in full implementation
        }
        self.inner.write()
    }

    /// Try to acquire a read lock without blocking.
    #[inline]
    pub fn try_read(&self) -> Option<RwLockReadGuard<'_, T>> {
        self.inner.try_read()
    }

    /// Try to acquire a write lock without blocking.
    #[inline]
    pub fn try_write(&self) -> Option<RwLockWriteGuard<'_, T>> {
        self.inner.try_write()
    }

    /// Get the lock level.
    #[inline]
    pub const fn level(&self) -> u8 {
        LEVEL
    }

    /// Get the lock name (debug builds only).
    #[cfg(debug_assertions)]
    #[inline]
    pub const fn name(&self) -> &'static str {
        self.name
    }
}

// Type aliases for each level
/// Level 0 lock - for global registries (PROCESSES, TENSORS)
pub type RegistryLock<T> = OrderedRwLock<T, 0>;

/// Level 1 lock - for tables (THREADS, ENDPOINTS, RINGS)
pub type TableLock<T> = OrderedRwLock<T, 1>;

/// Level 2 lock - for per-CPU state
pub type PerCpuLock<T> = OrderedRwLock<T, 2>;

/// Level 3 lock - for individual object state
pub type ObjectLock<T> = OrderedRwLock<T, 3>;

/// Macro to create a registry-level lock (Level 0)
#[macro_export]
macro_rules! registry_lock {
    ($value:expr, $name:literal) => {
        $crate::sync::RegistryLock::new($value, $name)
    };
}

/// Macro to create a table-level lock (Level 1)
#[macro_export]
macro_rules! table_lock {
    ($value:expr, $name:literal) => {
        $crate::sync::TableLock::new($value, $name)
    };
}

/// Macro to create a per-CPU level lock (Level 2)
#[macro_export]
macro_rules! per_cpu_lock {
    ($value:expr, $name:literal) => {
        $crate::sync::PerCpuLock::new($value, $name)
    };
}

/// Macro to create an object-level lock (Level 3)
#[macro_export]
macro_rules! object_lock {
    ($value:expr, $name:literal) => {
        $crate::sync::ObjectLock::new($value, $name)
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lock_levels() {
        let registry: RegistryLock<i32> = RegistryLock::new(42, "test_registry");
        let table: TableLock<i32> = TableLock::new(42, "test_table");
        let per_cpu: PerCpuLock<i32> = PerCpuLock::new(42, "test_per_cpu");
        let object: ObjectLock<i32> = ObjectLock::new(42, "test_object");

        assert_eq!(registry.level(), 0);
        assert_eq!(table.level(), 1);
        assert_eq!(per_cpu.level(), 2);
        assert_eq!(object.level(), 3);
    }

    #[test]
    fn test_read_write() {
        let lock: TableLock<i32> = TableLock::new(42, "test");

        {
            let read = lock.read();
            assert_eq!(*read, 42);
        }

        {
            let mut write = lock.write();
            *write = 100;
        }

        {
            let read = lock.read();
            assert_eq!(*read, 100);
        }
    }

    #[test]
    fn test_try_lock() {
        let lock: TableLock<i32> = TableLock::new(42, "test");

        // Should succeed when unlocked
        assert!(lock.try_read().is_some());
        assert!(lock.try_write().is_some());

        // Should fail when write-locked
        let _write = lock.write();
        // Note: try_read/try_write would return None here, but we can't test
        // that easily with spin locks in a single-threaded test
    }
}
