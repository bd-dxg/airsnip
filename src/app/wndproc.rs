// SPDX-License-Identifier: GPL-3.0-only
//! 消息窗口与窗口过程（SYS-1/2/3/4/6）：注册窗口类、创建隐藏窗口、分发消息。
//! 拆分自 `app`：`run` 入口与 `App` 状态在父模块，本文件只管窗口与消息路由。

use windows::Win32::Foundation::{GetLastError, GlobalFree, HGLOBAL, HINSTANCE, HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::Graphics::Gdi::HBRUSH;
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::{PCWSTR, w};

use super::{
    App, WINDOW_CLASS, WM_OVERLAY_DONE, WM_PIN_BORDER_CHANGED, WM_PIN_FROM_EDITOR, WM_TRAY, icon_resource,
    last_save_dir_take, last_save_ext_take, tray,
};

/// 注册消息窗口类并创建窗口（隐藏）。
pub(super) fn create_message_window(hinstance: HINSTANCE) -> Result<HWND, String> {
    unsafe {
        let icon = LoadIconW(Some(hinstance), icon_resource()).unwrap_or_default();
        let wc = WNDCLASSW {
            style: CS_HREDRAW | CS_VREDRAW,
            lpfnWndProc: Some(wndproc),
            hInstance: hinstance,
            hIcon: icon,
            hCursor: Default::default(),
            hbrBackground: HBRUSH::default(),
            lpszMenuName: PCWSTR::null(),
            lpszClassName: WINDOW_CLASS,
            ..Default::default()
        };
        if RegisterClassW(&wc) == 0 {
            return Err(format!("注册窗口类失败，错误码 {}", GetLastError().0));
        }

        let hwnd = match CreateWindowExW(
            WINDOW_EX_STYLE(0),
            WINDOW_CLASS,
            w!("jietu"),
            WINDOW_STYLE(0),
            0,
            0,
            0,
            0,
            None,
            None,
            Some(hinstance),
            None,
        ) {
            Ok(h) => h,
            Err(e) => return Err(format!("创建窗口失败：{e}")),
        };
        Ok(hwnd)
    }
}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    // explorer 启动/重启后任务栏通知区重建：重新添加托盘图标（开机自启早于 explorer 就绪时）。
    if tray::is_taskbar_created(msg) {
        if let Some(app) = app_from(hwnd) {
            app.init_tray();
        }
        return LRESULT(0);
    }
    match msg {
        WM_DESTROY => {
            unsafe { PostQuitMessage(0) };
            LRESULT(0)
        }
        WM_COMMAND => {
            let id = (wparam.0 & 0xFFFF) as u16;
            if let Some(app) = app_from(hwnd) {
                app.handle_command(id);
            }
            LRESULT(0)
        }
        WM_TRAY => {
            if let Some(app) = app_from(hwnd) {
                app.handle_tray(lparam);
            }
            LRESULT(0)
        }
        WM_HOTKEY => {
            let id = wparam.0 as i32;
            if let Some(app) = app_from(hwnd) {
                app.handle_hotkey(id);
            }
            LRESULT(0)
        }
        WM_PIN_FROM_EDITOR => {
            // 覆盖层线程送来的贴图载荷：由本（主）线程创建贴图窗口并释放内存
            let hg = HGLOBAL(wparam.0 as *mut core::ffi::c_void);
            let Some(app) = app_from(hwnd) else {
                // 主窗口异常（理论不可达）：释放载荷防泄漏
                unsafe {
                    let _ = GlobalFree(Some(hg));
                }
                return LRESULT(0);
            };
            unsafe { crate::pin::from_payload(hg, app.hinstance, app.config.pin_position, app.config.pin_border) };
            LRESULT(0)
        }
        WM_OVERLAY_DONE => {
            if let Some(app) = app_from(hwnd) {
                app.register_hotkeys(); // 重注册截图期间撤销的贴图热键
                // 本次保存用过对话框的话，记住格式与目录供下次默认（OUT-2）
                let mut changed = false;
                if let Some(ext) = last_save_ext_take() {
                    app.config.last_save_ext = Some(ext);
                    changed = true;
                }
                if let Some(dir) = last_save_dir_take() {
                    app.config.last_save_dir = Some(dir);
                    changed = true;
                }
                if changed {
                    app.config.save();
                }
            }
            LRESULT(0)
        }
        WM_PIN_BORDER_CHANGED => {
            // 贴图窗口内快捷键切了边框：更新默认配置（M2b）
            if let Some(app) = app_from(hwnd) {
                app.toggle_pin_border();
            }
            LRESULT(0)
        }
        WM_SETTINGCHANGE => {
            if let Some(app) = app_from(hwnd) {
                app.handle_setting_change(lparam);
            }
            unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
        }
        _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
    }
}

fn app_from(hwnd: HWND) -> Option<&'static mut App> {
    unsafe {
        let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA);
        if ptr == 0 { None } else { Some(&mut *(ptr as *mut App)) }
    }
}
