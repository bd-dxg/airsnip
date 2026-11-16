// SPDX-License-Identifier: GPL-3.0-only
//! 设置面板文本输入的消息处理（OUT-2 / ADR 0041）：键盘、字符与输入法。
//! 编辑态本身（字符数组 + 光标）与绘制在 `textbox`；这里只做「消息 → 编辑动作」的翻译。
//! 从 `textbox` 拆出，保证两个文件都在单文件规模上限内。

use super::Panel;
use super::textbox::{commit, sync_ime_pos};

/// 输入框相关消息的总入口（由面板窗口过程先调用）：返回 `true` 表示已消费，
/// 调用方只负责重绘并吞掉消息（不再进热键捕获或默认处理）。
/// 只接管 `WM_KEYDOWN` / `WM_SYSKEYDOWN` / `WM_CHAR` / `WM_IME_*`；其余消息（含 `WM_PAINT`）放行。
pub(super) fn on_message(panel: &mut Panel, msg: u32, wparam: usize, lparam: isize) -> bool {
    use windows::Win32::UI::WindowsAndMessaging::{
        WM_CHAR, WM_IME_COMPOSITION, WM_IME_STARTCOMPOSITION, WM_KEYDOWN, WM_SYSKEYDOWN,
    };
    if panel.editing.is_none() {
        return false;
    }
    match msg {
        WM_KEYDOWN | WM_SYSKEYDOWN => {
            on_key(panel, wparam as u32, ctrl_down());
            true
        }
        WM_CHAR => {
            if let Some(ch) = char::from_u32(wparam as u32) {
                on_char(panel, ch);
            }
            true
        }
        WM_IME_STARTCOMPOSITION => {
            // 输入法正要显示候选框：先把锚点挪到自绘光标（issue #139 的同款时机）
            sync_ime_pos(panel);
            true
        }
        WM_IME_COMPOSITION => {
            on_composition(panel, lparam as u32);
            true
        }
        _ => false,
    }
}

/// Ctrl 是否按下（面板侧判断，与覆盖层 `keys` 同一套 `GetKeyState` 读法）。
fn ctrl_down() -> bool {
    use windows::Win32::UI::Input::KeyboardAndMouse::GetKeyState;
    unsafe { GetKeyState(0x11) as u16 & 0x8000 != 0 }
}

/// 键盘处理：返回 `true` 表示已消费（调用方不再分发）。
/// Esc / Enter 提交并退出编辑；方向键 / Home / End / 退格 / 删除原地处理；Ctrl+V 粘贴。
fn on_key(panel: &mut Panel, vk: u32, ctrl: bool) -> bool {
    if panel.editing.is_none() {
        return false;
    }
    match vk {
        0x1B | 0x0D => {
            commit(panel);
            return true;
        }
        0x56 if ctrl => {
            if let Some(text) = crate::output::read_text_from_clipboard()
                && let Some(e) = panel.editing.as_mut()
            {
                e.insert_str(&text);
            }
        }
        _ => {
            if let Some(e) = panel.editing.as_mut() {
                match vk {
                    0x08 => e.backspace(),
                    0x2E => e.delete(),
                    0x25 => e.left(),
                    0x27 => e.right(),
                    0x24 => e.home(),
                    0x23 => e.end(),
                    // 其它键（含修饰键本身）一律吞掉：编辑期间不应触发面板快捷键
                    _ => {}
                }
            }
        }
    }
    sync_ime_pos(panel);
    true
}

/// 字符输入（`WM_CHAR`）：可打印字符才收；控制字符由 `on_key` 处理过了，这里只吞掉。
fn on_char(panel: &mut Panel, ch: char) -> bool {
    let Some(e) = panel.editing.as_mut() else {
        return false;
    };
    if !ch.is_control() {
        e.insert_str(&ch.to_string());
    }
    sync_ime_pos(panel);
    true
}

/// 输入法结果串（`WM_IME_COMPOSITION` + `GCS_RESULTSTR`）：插到光标处。
/// 组合过程串不预览（面板没有自绘候选框，交由输入法自己的候选窗显示）。
fn on_composition(panel: &mut Panel, flags: u32) -> bool {
    use windows::Win32::UI::Input::Ime::{GCS_RESULTSTR, ImmGetCompositionStringW, ImmGetContext, ImmReleaseContext};
    if panel.editing.is_none() {
        return false;
    }
    if flags & GCS_RESULTSTR.0 == 0 {
        return true;
    }
    let mut text = String::new();
    unsafe {
        let ctx = ImmGetContext(panel.hwnd);
        if !ctx.0.is_null() {
            let len = ImmGetCompositionStringW(ctx, GCS_RESULTSTR, None, 0);
            if len > 0 {
                let mut buf = vec![0u16; (len / 2) as usize];
                ImmGetCompositionStringW(
                    ctx,
                    GCS_RESULTSTR,
                    Some(buf.as_mut_ptr() as *mut core::ffi::c_void),
                    len as u32,
                );
                text = String::from_utf16_lossy(&buf);
            }
            let _ = ImmReleaseContext(panel.hwnd, ctx);
        }
    }
    if let Some(e) = panel.editing.as_mut() {
        e.insert_str(&text);
    }
    sync_ime_pos(panel);
    true
}
