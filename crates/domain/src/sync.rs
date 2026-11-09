//! UIDM's one policy for poisoned locks (AGENTS.md section 9): take the
//! guard back and go on.
//!
//! A lock is poisoned when a thread panicked while holding it. Recovering is
//! right for UIDM because:
//!
//! - Every lock guards plain data (a list, a map, a set, settings, a flag),
//!   changed by single inserts, removals or assignments. Rust keeps that data
//!   memory-safe whatever the panic, so the worst a poisoned lock holds is
//!   the last change half made, never a value that cannot be used.
//! - The panic was already logged: each program's `logging::init` hooks
//!   panics into its log file.
//! - The other choice, panicking again on every later lock, turns one bug in
//!   one download's thread into the loss of the daemon and every download it
//!   runs, or of the window.
//!
//! The helpers are pure `std` and do no I/O, so they live in the domain, the
//! one crate every layer may depend on. Use them for every `Mutex::lock`,
//! `RwLock::read` and `RwLock::write`, and [`recover`] for anything else that
//! returns a [`LockResult`], such as a `Condvar` wait.

use std::sync::{LockResult, Mutex, MutexGuard, PoisonError, RwLock, RwLockReadGuard, RwLockWriteGuard};

/// Take back the guard of a lock a panicking thread poisoned; see the
/// module's policy.
pub fn recover<G>(result: LockResult<G>) -> G {
    result.unwrap_or_else(PoisonError::into_inner)
}

/// Lock a [`Mutex`], recovering it if poisoned (see the module's policy).
pub trait LockExt<T: ?Sized> {
    /// Block until the lock is held, and hold it even if poisoned.
    fn lock_or_recover(&self) -> MutexGuard<'_, T>;
}

impl<T: ?Sized> LockExt<T> for Mutex<T> {
    fn lock_or_recover(&self) -> MutexGuard<'_, T> {
        recover(self.lock())
    }
}

/// Read or write an [`RwLock`], recovering it if poisoned (see the module's
/// policy).
pub trait RwLockExt<T: ?Sized> {
    /// Block until the lock is shared, even if poisoned.
    fn read_or_recover(&self) -> RwLockReadGuard<'_, T>;
    /// Block until the lock is held alone, even if poisoned.
    fn write_or_recover(&self) -> RwLockWriteGuard<'_, T>;
}

impl<T: ?Sized> RwLockExt<T> for RwLock<T> {
    fn read_or_recover(&self) -> RwLockReadGuard<'_, T> {
        recover(self.read())
    }

    fn write_or_recover(&self) -> RwLockWriteGuard<'_, T> {
        recover(self.write())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::thread;

    fn poisoned<T: Send + 'static>(value: T) -> Arc<Mutex<T>> {
        let m = Arc::new(Mutex::new(value));
        let held = Arc::clone(&m);
        let _ = thread::spawn(move || {
            let _guard = held.lock().unwrap();
            panic!("poison it");
        })
        .join();
        m
    }

    #[test]
    fn a_poisoned_mutex_still_locks_with_its_value() {
        let m = poisoned(vec![1, 2]);
        assert!(m.is_poisoned());
        m.lock_or_recover().push(3);
        assert_eq!(*m.lock_or_recover(), vec![1, 2, 3]);
    }

    #[test]
    fn a_poisoned_rwlock_still_reads_and_writes() {
        let l = Arc::new(RwLock::new(1));
        let held = Arc::clone(&l);
        let _ = thread::spawn(move || {
            let _guard = held.write().unwrap();
            panic!("poison it");
        })
        .join();
        assert!(l.is_poisoned());
        *l.write_or_recover() += 1;
        assert_eq!(*l.read_or_recover(), 2);
    }
}
