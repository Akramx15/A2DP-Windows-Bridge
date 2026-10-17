//! One running instance: a named mutex
//!
//! v1 (GUI mode) creates the mutex "A2DPWB_SingleInstance" at start and
//! quits if it already exists (main.cpp). v2 uses its own mutex the same
//! way, and checks v1's with `is_held` to tell the user that v1 still runs
//! (both cannot use the same USB adapter).

use windows::Win32::Foundation::{CloseHandle, ERROR_ACCESS_DENIED, ERROR_ALREADY_EXISTS, GetLastError, HANDLE};
use windows::Win32::System::Threading::{CreateMutexW, OpenMutexW, SYNCHRONIZATION_SYNCHRONIZE};
use windows::core::HSTRING;

/// The mutex v1 holds while its GUI runs
pub const V1_MUTEX_NAME: &str = "A2DPWB_SingleInstance";

/// Holds the named mutex; another `acquire` of the same name fails while
/// this lives. Released on drop (and by Windows when the process ends).
#[derive(Debug)]
pub struct SingleInstance {
    handle: HANDLE,
}

// SAFETY: a mutex handle is a kernel object reference usable from any
// thread; the guard is never used for locking, only for its existence.
unsafe impl Send for SingleInstance {}
// SAFETY: no method uses the handle through &self.
unsafe impl Sync for SingleInstance {}

impl SingleInstance {
    /// Creates the mutex `name` (session-local, like v1's). Ok(None) if it
    /// already exists, i.e. another instance runs.
    pub fn acquire(name: &str) -> windows::core::Result<Option<SingleInstance>> {
        // Not taking ownership: only the mutex's existence matters, and an
        // owned mutex would be "abandoned" when the creating thread ends
        // SAFETY: default security, NUL-terminated name.
        let handle = unsafe { CreateMutexW(None, false, &HSTRING::from(name)) }?;
        // SAFETY: reads this thread's last error, set by CreateMutexW just
        // above (it reports an existing mutex with a valid handle).
        if unsafe { GetLastError() } == ERROR_ALREADY_EXISTS {
            // SAFETY: the handle was just opened here.
            let _ = unsafe { CloseHandle(handle) };
            return Ok(None);
        }
        Ok(Some(SingleInstance { handle }))
    }
}

impl Drop for SingleInstance {
    fn drop(&mut self) {
        // SAFETY: the handle is owned by this guard and closed only here.
        let _ = unsafe { CloseHandle(self.handle) };
    }
}

/// True if a mutex `name` exists, i.e. the program holding it runs. A
/// mutex that exists but cannot be opened (another user, higher integrity)
/// counts as held.
pub fn is_held(name: &str) -> bool {
    // SAFETY: NUL-terminated name; the handle is closed right away.
    match unsafe { OpenMutexW(SYNCHRONIZATION_SYNCHRONIZE, false, &HSTRING::from(name)) } {
        Ok(handle) => {
            // SAFETY: just opened here.
            let _ = unsafe { CloseHandle(handle) };
            true
        }
        Err(e) => e.code() == ERROR_ACCESS_DENIED.to_hresult(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn second_acquire_fails_until_released() {
        let name = format!("A2DPWB_test_single_instance_{}", std::process::id());
        assert!(!is_held(&name));
        let first = SingleInstance::acquire(&name).unwrap().expect("first instance");
        assert!(is_held(&name));
        assert!(SingleInstance::acquire(&name).unwrap().is_none());
        drop(first);
        assert!(!is_held(&name));
        assert!(SingleInstance::acquire(&name).unwrap().is_some());
    }

    #[test]
    fn v1_check_runs() {
        // Read-only; v1 may or may not be running here
        let _ = is_held(V1_MUTEX_NAME);
    }
}
