// SPDX-License-Identifier: GPL-3.0-only
//! 托盘图标与右键菜单（SYS-1）。
//! 右键菜单为自绘弹出窗口（见 `popmenu`）：无边框 + 阴影、单击选中 / 双击执行，
//! 颜色跟随主题调色板（系统原生菜单固定跟随 Windows 主题，深色设置下仍白底且带边框）。

use windows::Win32::Foundation::{GetLastError, LPARAM, WPARAM};
use windows::Win32::UI::Shell::{
    NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, NIM_DELETE, NOTIFYICONDATAW, Shell_NotifyIconW,
};
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::w;

use super::{App, ID_TRAY, IDM_EXIT, IDM_PIN, IDM_SETTINGS, IDM_SHOT, WM_TRAY, icon_resource, popmenu, wide_array};
use std::mem::size_of;
use std::sync::OnceLock;

/// `TaskbarCreated` 广播消息 ID（惰性注册，会话内全局唯一）。
static TASKBAR_CREATED: OnceLock<u32> = OnceLock::new();

/// 托盘图标补加定时器 ID（`WM_TIMER` 里按它识别，SYS-1）。
pub(super) const TIMER_TRAY_RETRY: usize = 0x7A01;
/// 补加次数上限与间隔：explorer 刚启动时通知区可能未就绪，NIM_ADD 会失败。
/// 5 × 1s = 5s，低配机 explorer 重建通知区也能赶上。
const TRAY_RETRY_MAX: u8 = 5;
const TRAY_RETRY_MS: u32 = 1000;

/// `TaskbarCreated` 注册消息 ID（0 表示注册失败）。
/// 同时供 `wndproc` 用 `ChangeWindowMessageFilterEx` 放行 UIPI 拦截。
pub fn taskbar_created_message() -> u32 {
    *TASKBAR_CREATED.get_or_init(|| unsafe { RegisterWindowMessageW(w!("TaskbarCreated")) })
}

/// 是否为 `TaskbarCreated` 广播：explorer 启动或重启后任务栏通知区重建。
/// 开机自启早于 explorer 就绪时首次 `NIM_ADD` 会失败且不报错，收到该广播时补加图标（SYS-1）。
pub fn is_taskbar_created(msg: u32) -> bool {
    let id = taskbar_created_message();
    id != 0 && msg == id
}

impl App {
    /// 添加托盘图标（SYS-1），返回是否成功。私有：外部一律走 `ensure_tray` / `restore_tray`。
    fn init_tray(&self) -> bool {
        let nid = NOTIFYICONDATAW {
            cbSize: size_of::<NOTIFYICONDATAW>() as u32,
            hWnd: self.hwnd,
            uID: ID_TRAY,
            uFlags: NIF_MESSAGE | NIF_ICON | NIF_TIP,
            uCallbackMessage: WM_TRAY,
            hIcon: unsafe { LoadIconW(Some(self.hinstance), icon_resource()).unwrap_or_default() },
            szTip: wide_array("AirSnip 轻截"),
            ..Default::default()
        };
        let ok = unsafe { Shell_NotifyIconW(NIM_ADD, &nid) }.as_bool();
        if !ok && std::env::var_os("AIRSNIP_DEBUG").is_some() {
            eprintln!("[airsnip] 托盘 NIM_ADD 失败，错误码 {:?}", unsafe {
                GetLastError()
            });
        }
        ok
    }

    /// 确保托盘图标存在（SYS-1）：失败时用定时器补加，成功则停掉定时器。
    /// 首次启动、explorer 重启（`TaskbarCreated`）与休眠唤醒（`WM_POWERBROADCAST`）都走这里。
    pub fn ensure_tray(&mut self) {
        if self.init_tray() {
            self.tray_retry = 0;
            unsafe {
                let _ = KillTimer(Some(self.hwnd), TIMER_TRAY_RETRY);
            }
            return;
        }
        if self.tray_retry >= TRAY_RETRY_MAX {
            return;
        }
        self.tray_retry += 1;
        unsafe { SetTimer(Some(self.hwnd), TIMER_TRAY_RETRY, TRAY_RETRY_MS, None) };
    }

    /// 通知区重建后重加托盘图标：清零重试计数再走 `ensure_tray`。
    /// 供 `TaskbarCreated` 与休眠唤醒使用；定时器重试本身必须直接调 `ensure_tray`，
    /// 否则计数被反复清零会无限重试。
    pub fn restore_tray(&mut self) {
        self.tray_retry = 0;
        self.ensure_tray();
    }

    /// 移除托盘图标。
    pub fn remove_tray(&self) {
        let nid = NOTIFYICONDATAW {
            cbSize: size_of::<NOTIFYICONDATAW>() as u32,
            hWnd: self.hwnd,
            uID: ID_TRAY,
            ..Default::default()
        };
        unsafe {
            let _ = Shell_NotifyIconW(NIM_DELETE, &nid);
        }
    }

    /// 托盘回调（SYS-1）：左键打开设置，右键弹出菜单。
    pub fn handle_tray(&mut self, lparam: LPARAM) {
        let msg = (lparam.0 & 0xFFFF) as u16;
        if u32::from(msg) == WM_LBUTTONUP {
            self.open_settings();
        } else if u32::from(msg) == WM_RBUTTONUP {
            self.show_tray_menu();
        }
    }

    /// 右键托源自绘菜单：截图 / 从剪贴板贴图 / 设置 / 退出。
    /// 截图与贴图不只有全局热键入口：热键被用户清空（SYS-2）时仍能从菜单触发。
    fn show_tray_menu(&mut self) {
        let items = vec![
            popmenu::Item {
                id: IDM_SHOT,
                text: "截图",
                separator_before: false,
            },
            popmenu::Item {
                id: IDM_PIN,
                text: "从剪贴板贴图",
                separator_before: false,
            },
            popmenu::Item {
                id: IDM_SETTINGS,
                text: "设置...",
                separator_before: true,
            },
            popmenu::Item {
                id: IDM_EXIT,
                text: "退出",
                separator_before: false,
            },
        ];
        let palette = crate::theme::palette(crate::theme::resolve(self.config.theme));
        if let Some(id) = popmenu::popup(self.hwnd, items, palette) {
            self.handle_command(id as u16);
        }
    }

    /// 菜单项分发。
    pub fn handle_command(&mut self, id: u16) {
        match id as usize {
            IDM_SHOT => self.start_capture(),
            // 热键被清空时的贴图退路（PIN-2）
            IDM_PIN => crate::pin::from_clipboard(self.hinstance, self.config.pin_position, self.config.pin_border),
            IDM_SETTINGS => self.open_settings(),
            IDM_EXIT => unsafe {
                let _ = PostMessageW(Some(self.hwnd), WM_CLOSE, WPARAM(0), LPARAM(0));
            },
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 注册得到的 TaskbarCreated 消息 ID 能被识别，普通消息不会误判。
    #[test]
    fn taskbar_created_message_is_recognized() {
        let id = unsafe { RegisterWindowMessageW(w!("TaskbarCreated")) };
        assert_ne!(id, 0, "TaskbarCreated 应能注册成功");
        assert!(is_taskbar_created(id), "注册消息 ID 应被识别为 TaskbarCreated");
        assert!(!is_taskbar_created(WM_TRAY), "托盘回调消息不应误判");
    }
}
