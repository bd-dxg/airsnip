// SPDX-License-Identifier: GPL-3.0-only
//! 键盘 / IME 辅助（CAP-2、EDT-1·EDT-2、EDT-4、ADR 0023）：修饰键状态与 IME 组合串读取。
//! 从 `wndproc` 拆出，让窗口过程只留消息分发，单文件守在 300 行以内。
//! 调用方仍按 `super::wndproc::xxx` 引用（本模块在 `wndproc/mod.rs` 里 `pub(super) use` 转出）。

use windows::Win32::Foundation::HWND;
use windows::Win32::UI::Input::Ime::IME_COMPOSITION_STRING;
use windows::Win32::UI::Input::KeyboardAndMouse::GetKeyState;

/// 修饰键是否按下（`GetKeyState` 高位为 1 表示按下）。
fn key_down(vk: i32) -> bool {
    unsafe { GetKeyState(vk) as u16 & 0x8000 != 0 }
}

/// Ctrl 是否按下（Ctrl + 拖动对象本体 = 移动该对象，ADR 0023）。
pub(in crate::overlay) fn ctrl_down() -> bool {
    key_down(0x11)
}

/// Shift 是否按下（文本输入修饰键 / 绘制约束 EDT-1·EDT-2）。
pub(in crate::overlay) fn shift_down() -> bool {
    key_down(0x10)
}

/// 空格键是否按下（按住空格拖动可移动整个选区，CAP-2）。
pub(in crate::overlay) fn space_down() -> bool {
    key_down(0x20)
}

/// 鼠标左键是否按住（窗口吸附的「候选待定」靠它发现丢失的 `WM_LBUTTONUP`）。
pub(in crate::overlay) fn lbutton_down() -> bool {
    key_down(0x01)
}

/// 从 IME 上下文读取组合串/结果串（UTF-16 → String，EDT-4）。
pub(in crate::overlay) fn ime_string(hwnd: HWND, what: IME_COMPOSITION_STRING) -> String {
    unsafe {
        use windows::Win32::UI::Input::Ime::{ImmGetCompositionStringW, ImmGetContext, ImmReleaseContext};
        let ctx = ImmGetContext(hwnd);
        if ctx.0.is_null() {
            return String::new();
        }
        let len = ImmGetCompositionStringW(ctx, what, None, 0);
        let out = if len <= 0 {
            String::new()
        } else {
            let mut buf = vec![0u16; (len / 2) as usize];
            ImmGetCompositionStringW(ctx, what, Some(buf.as_mut_ptr() as *mut core::ffi::c_void), len as u32);
            String::from_utf16_lossy(&buf)
        };
        let _ = ImmReleaseContext(hwnd, ctx);
        out
    }
}
