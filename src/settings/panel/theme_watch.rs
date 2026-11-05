// SPDX-License-Identifier: GPL-3.0-only
//! 系统主题变化监听（SYS-6）：注册表通知线程，变化时给设置面板投递消息。
//!
//! 不用 `WM_SETTINGCHANGE` 做唯一入口：AirSnip 以管理员权限运行，而深/浅色切换的
//! 广播通常由用户级进程（设置应用 / explorer）发出，会被 UIPI 挡在高完整性窗口之外，
//! 表现为「选了跟随系统，切主题面板毫无反应」。
//! 注册表通知不经过窗口消息过滤，且 `RegNotifyChangeKeyValue` 是事件驱动的
//! （线程阻塞在 `WaitForSingleObject`），不违反「待机 0% CPU、无轮询」红线。

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use windows::Win32::Foundation::{CloseHandle, ERROR_SUCCESS, HANDLE, HWND, LPARAM, WAIT_OBJECT_0, WPARAM};
use windows::Win32::System::Registry::{
    HKEY, HKEY_CURRENT_USER, KEY_NOTIFY, REG_NOTIFY_CHANGE_LAST_SET, RegCloseKey, RegNotifyChangeKeyValue,
    RegOpenKeyExW,
};
use windows::Win32::System::Threading::{CreateEventW, INFINITE, SetEvent, WaitForSingleObject};
use windows::Win32::UI::WindowsAndMessaging::{PostMessageW, WM_APP};
use windows::core::{PCWSTR, w};

/// 主题已变化的通知消息（面板窗口过程里处理）。
pub(super) const WM_THEME_CHANGED: u32 = WM_APP + 0x51;

const PERSONALIZE: PCWSTR = w!("Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize");

/// 监听器：持有等待事件与线程句柄，drop 时唤醒线程并等它退出。
pub(super) struct ThemeWatch {
    stop: Arc<AtomicBool>,
    wake: HANDLE,
    join: Option<std::thread::JoinHandle<()>>,
}

impl ThemeWatch {
    /// 开始监听；`hwnd` 收到 `WM_THEME_CHANGED` 时应重新解析主题并重绘。
    pub(super) fn spawn(hwnd: HWND) -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let wake = unsafe { CreateEventW(None, false, false, None) }.unwrap_or_default();
        let thread_stop = Arc::clone(&stop);
        let thread_wake = wake.0 as isize; // HANDLE 非 Send，跨线程传原生值
        let target = hwnd.0 as isize; // HWND 非 Send，跨线程传原生值
        let join = std::thread::spawn(move || watch(target, thread_stop, thread_wake));
        Self {
            stop,
            wake,
            join: Some(join),
        }
    }
}

impl Drop for ThemeWatch {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        unsafe {
            let _ = SetEvent(self.wake);
        }
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
        unsafe {
            let _ = CloseHandle(self.wake);
        }
    }
}

/// 线程主体：注册变更通知 → 等待 → 投递消息，循环到 `stop`。
/// 通知是一次性的（触发后须重新注册），所以放在循环里。
fn watch(hwnd: isize, stop: Arc<AtomicBool>, wake_raw: isize) {
    let wake = HANDLE(wake_raw as *mut core::ffi::c_void);
    let mut key = HKEY::default();
    if unsafe { RegOpenKeyExW(HKEY_CURRENT_USER, PERSONALIZE, None, KEY_NOTIFY, &mut key) } != ERROR_SUCCESS {
        return; // 注册表不可读：退化为只依赖 WM_SETTINGCHANGE
    }
    while !stop.load(Ordering::Acquire) {
        let notify = unsafe { RegNotifyChangeKeyValue(key, true, REG_NOTIFY_CHANGE_LAST_SET, Some(wake), true) };
        if notify != ERROR_SUCCESS {
            break;
        }
        if unsafe { WaitForSingleObject(wake, INFINITE) } != WAIT_OBJECT_0 {
            break;
        }
        if stop.load(Ordering::Acquire) {
            break;
        }
        // 面板可能已销毁：投递失败无害
        unsafe {
            let _ = PostMessageW(Some(HWND(hwnd as *mut _)), WM_THEME_CHANGED, WPARAM(0), LPARAM(0));
        }
    }
    unsafe {
        let _ = RegCloseKey(key);
    }
}
