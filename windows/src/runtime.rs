use std::sync::atomic::{AtomicBool, Ordering};

use anyhow::{Context, Result, bail};
use windows::Win32::Foundation::{CloseHandle, ERROR_ALREADY_EXISTS, GetLastError, HANDLE};
use windows::Win32::System::Console::{
    CTRL_BREAK_EVENT, CTRL_C_EVENT, CTRL_CLOSE_EVENT, CTRL_LOGOFF_EVENT, CTRL_SHUTDOWN_EVENT,
    SetConsoleCtrlHandler,
};
use windows::Win32::System::Threading::CreateMutexW;
use windows_core::{BOOL, w};

static SHUTDOWN_REQUESTED: AtomicBool = AtomicBool::new(false);

pub struct SingleInstance(HANDLE);

impl SingleInstance {
    pub fn acquire() -> Result<Self> {
        // SAFETY: the name is a static null-terminated string and the returned handle is owned here.
        let handle = unsafe {
            CreateMutexW(
                None,
                false,
                w!("Local\\AudioSelector.Windows.Application.v1"),
            )
        }
        .context("多重起動防止Mutexの作成に失敗しました")?;
        // SAFETY: GetLastError reads the calling thread's last Win32 error immediately after creation.
        if unsafe { GetLastError() } == ERROR_ALREADY_EXISTS {
            // SAFETY: this process owns the returned handle even when the object already existed.
            let _ = unsafe { CloseHandle(handle) };
            bail!("Audio Selectorはすでに起動しています");
        }
        Ok(Self(handle))
    }
}

impl Drop for SingleInstance {
    fn drop(&mut self) {
        // SAFETY: this handle was created once in acquire and is closed once here.
        let _ = unsafe { CloseHandle(self.0) };
    }
}

pub fn install_shutdown_handler() -> Result<()> {
    SHUTDOWN_REQUESTED.store(false, Ordering::SeqCst);
    // SAFETY: the handler only updates a lock-free atomic and remains valid for process lifetime.
    unsafe { SetConsoleCtrlHandler(Some(console_control_handler), true) }
        .context("Console終了ハンドラーの登録に失敗しました")
}

pub fn shutdown_requested() -> bool {
    SHUTDOWN_REQUESTED.load(Ordering::SeqCst)
}

unsafe extern "system" fn console_control_handler(control_type: u32) -> BOOL {
    if matches!(
        control_type,
        CTRL_C_EVENT
            | CTRL_BREAK_EVENT
            | CTRL_CLOSE_EVENT
            | CTRL_LOGOFF_EVENT
            | CTRL_SHUTDOWN_EVENT
    ) {
        SHUTDOWN_REQUESTED.store(true, Ordering::SeqCst);
        true.into()
    } else {
        false.into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ctrl_c_requests_graceful_shutdown() {
        SHUTDOWN_REQUESTED.store(false, Ordering::SeqCst);
        // SAFETY: this directly exercises the registered callback with a documented event value.
        assert!(unsafe { console_control_handler(CTRL_C_EVENT) }.as_bool());
        assert!(shutdown_requested());
        SHUTDOWN_REQUESTED.store(false, Ordering::SeqCst);
    }
}
