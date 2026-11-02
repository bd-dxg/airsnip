// SPDX-License-Identifier: GPL-3.0-only
//! 界面唤醒（SYS-7）：后台线程完成检查/下载后，让设置面板刷新「关于」页状态。
//!
//! 面板是模态消息循环，后台线程不直接触碰 `Panel`（跨线程访问会踩内存），
//! 只投递一个自定义消息，由面板自己的 `wndproc` 重绘。

use std::sync::OnceLock;

use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::PostMessageW;

/// 通知更新状态变化（面板 `wndproc` 收到后重绘关于页）。
pub const WM_UPDATE_STATE: u32 = windows::Win32::UI::WindowsAndMessaging::WM_APP + 20;
/// 发现新版本（主窗口收到后弹托盘气泡）。
pub const WM_UPDATE_FOUND: u32 = windows::Win32::UI::WindowsAndMessaging::WM_APP + 21;

/// 登记需要接收通知的窗口（设置面板打开时登记，关闭时清空）。
fn slot() -> &'static std::sync::Mutex<Option<isize>> {
    static SLOT: OnceLock<std::sync::Mutex<Option<isize>>> = OnceLock::new();
    SLOT.get_or_init(|| std::sync::Mutex::new(None))
}

/// 登记通知目标（`HWND` 不是 `Send`，只存原生指针值）。
pub fn register(hwnd: HWND) {
    if let Ok(mut g) = slot().lock() {
        *g = Some(hwnd.0 as isize);
    }
}

/// 取消登记（面板关闭时调用，避免向已销毁的窗口投递消息）。
pub fn unregister() {
    if let Ok(mut g) = slot().lock() {
        *g = None;
    }
}

/// 唤醒界面线程：有登记目标才发，投递失败静默（面板可能正在关闭）。
pub fn wake_ui() {
    let target = slot().lock().ok().and_then(|g| *g);
    if let Some(raw) = target {
        unsafe {
            let _ = PostMessageW(Some(HWND(raw as *mut _)), WM_UPDATE_STATE, WPARAM(0), LPARAM(0));
        }
    }
}

/// 主窗口句柄（启动时登记，用于发现新版本时弹托盘气泡）。
fn main_slot() -> &'static std::sync::Mutex<Option<isize>> {
    static MAIN: OnceLock<std::sync::Mutex<Option<isize>>> = OnceLock::new();
    MAIN.get_or_init(|| std::sync::Mutex::new(None))
}

/// 登记主窗口（`app::run` 启动时调一次）。
pub fn register_main(hwnd: HWND) {
    if let Ok(mut g) = main_slot().lock() {
        *g = Some(hwnd.0 as isize);
    }
}

/// 提示主窗口「发现了新版本」（由它弹托盘气泡）。
pub fn wake_main_on_update() {
    let target = main_slot().lock().ok().and_then(|g| *g);
    if let Some(raw) = target {
        unsafe {
            let _ = PostMessageW(Some(HWND(raw as *mut _)), WM_UPDATE_FOUND, WPARAM(0), LPARAM(0));
        }
    }
}
