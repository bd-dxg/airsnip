// SPDX-License-Identifier: GPL-3.0-only
//! 覆盖层键盘分发（`WM_KEYDOWN` / `WM_SYSKEYDOWN`）：从 `wndproc` 拆出（ADR 0041 把那边撑过 300 行）。
//!
//! 两条消息的判定顺序是同一套：**保存组合键 → 空格 → 动作键 → 微调 / 工具切换**。
//! `Alt+键` 走 `WM_SYSKEYDOWN`（普通键才走 `WM_KEYDOWN`，见 issue #90），
//! 因此保存 / 快速保存（`Ctrl+S` / `Alt+S`）必须在两条路径上都判一遍。

use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::UI::Input::Ime::ImmGetVirtualKey;
use windows::Win32::UI::WindowsAndMessaging::DefWindowProcW;

use super::super::Overlay;
use super::super::cancel_hint;
use super::savekeys;
use super::{ctrl_down, shift_down};

/// `WM_KEYDOWN`：覆盖层内的大部分按键。
pub(super) fn on_keydown(overlay: &mut Overlay, hwnd: HWND, wparam: WPARAM) -> LRESULT {
    // 文本输入中：只处理输入相关按键（光标移动/退格/Enter 提交/Esc 取消），
    // 拦截全局快捷键与空格抓手切换（空格作为普通字符经 WM_CHAR 进入文本）
    if overlay.is_editing_text() {
        overlay.on_text_key(wparam.0 as u32, shift_down(), ctrl_down());
        return LRESULT(0);
    }
    // 中文输入法活动时，按键会被改写为 VK_PROCESSKEY(0xE5)，真实键另存。
    // 不禁用 IME（文本工具需要），改用 ImmGetVirtualKey 取回真实 vk，
    // 否则 IME 开启状态下按 S/F3/回车等全部失效（M4 实测缺陷）。
    let vk = if wparam.0 as u32 == 0xE5 {
        unsafe { ImmGetVirtualKey(hwnd) }
    } else {
        wparam.0 as u32
    };
    // 除 Esc 外的按键都解除取消待命（Esc 本身走下面的取消分支）
    if vk != 0x1B {
        overlay.clear_cancel_arm();
    }
    // 保存 / 快速保存（OUT-2 / ADR 0041）：带修饰键的组合键，**先于**无修饰键的覆盖层键判定
    // （否则 `Shift+S` 之类会被长截图键抢走）。提示可见期间只接受「收尾」。
    if savekeys::handle(overlay, vk, false) {
        return LRESULT(0);
    }
    // 空格按下立即刷新光标：系统只在鼠标移动时发 WM_SETCURSOR，不能等它。
    // 贴图编辑模式（ADR 0028）下空格 = 完成（选区锁定，无需空格移动选区）。
    if vk == 0x20 {
        if overlay.locked_selection {
            overlay.finish_output();
        } else {
            overlay.apply_cursor_now(hwnd);
        }
        return LRESULT(0);
    }
    match vk {
        // Esc：与右键共用分层取消（先收面板 / 取消输入；有标注则二次确认，OUT-3/ADR 0027）
        0x1B => overlay.on_cancel(cancel_hint::KEY_ESCAPE),
        0x0D => overlay.finish_output(), // Enter：普通=复制关闭，编辑=写回贴图（ADR 0028）
        0x43 if ctrl_down() => overlay.on_copy(), // Ctrl+C
        // PIN-1：贴选区（键可配，默认 F3）；编辑模式下输出组不适用
        k if k == overlay.pin_key && !overlay.locked_selection => overlay.on_pin(),
        // 长截图键（默认 S，设置面板可改）：当前选区作为滚动视口（M4）
        k if k == overlay.longshot_key && !overlay.locked_selection => overlay.on_longshot(),
        // 快速选区键（ADR 0040，默认 F / R）：仅在没有选区时生效，
        // 有效条件（选区 / 贴图编辑模式）在方法内部判
        k if k == overlay.fullscreen_key && !overlay.locked_selection => overlay.on_select_fullscreen(),
        k if k == overlay.last_area_key && !overlay.locked_selection => overlay.on_select_last_area(),
        0x5A if ctrl_down() => {
            // Ctrl+Z 撤销 / Ctrl+Shift+Z 重做（EDT-7）
            if shift_down() {
                overlay.on_redo()
            } else {
                overlay.on_undo()
            }
        }
        0x59 if ctrl_down() => overlay.on_redo(), // Ctrl+Y
        // 删除选中对象（issue #71 / ADR 0032）：`Del` 与 `Backspace` 硬编码。
        // 仅在真的删掉时消费按键，否则落到下面的工具切换——
        // 这两个键可被用户绑成工具键（不在冲突检测表里，没选中对象时照旧生效）。
        0x2E | 0x08 if overlay.on_delete_selected() => {}
        _ => {
            // 方向键微调（EDT-7 / CAP-2）：1px，Shift 5px；优先于工具切换键
            if let Some((dx, dy)) = crate::app::hotkey::arrow_step(vk, shift_down()) {
                overlay.nudge(dx, dy);
                return LRESULT(0);
            }
            // 工具切换键（M3）：任意键都查映射（不限于数字键），
            // 以便设置面板可把工具键改为字母等
            if let Some(m) = overlay.mode_by_key(vk) {
                overlay.switch_mode(m);
            }
        }
    }
    LRESULT(0)
}

/// `WM_SYSKEYDOWN`：`F10` 是系统菜单键、`Alt+键` 也走这里。
/// 只接管动作键与保存组合，其余透传保留 `Alt+F4` / `Alt+Tab` 等系统组合。
pub(super) fn on_syskeydown(overlay: &mut Overlay, hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    let vk = wparam.0 as u32;
    let alt_down = (lparam.0 as u32 >> 29) & 1 != 0;
    // 保存 / 快速保存（ADR 0041）默认就带 Alt（`Alt+S`），必须在 Alt 按下时也接管；
    // 非 Alt 的系统键（`F10` 等）也先过一遍组合判定（不匹配就照旧往下走）。
    if !overlay.is_editing_text() && savekeys::handle(overlay, vk, alt_down) {
        return LRESULT(0);
    }
    let action = !overlay.locked_selection
        && (vk == overlay.pin_key
            || vk == overlay.longshot_key
            || vk == overlay.fullscreen_key
            || vk == overlay.last_area_key);
    if alt_down || overlay.is_editing_text() || !action {
        return unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) };
    }
    overlay.on_aux_key(vk);
    LRESULT(0)
}
