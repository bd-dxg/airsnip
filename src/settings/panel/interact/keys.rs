// SPDX-License-Identifier: GPL-3.0-only
//! 设置面板键盘交互（M3）：热键捕获、冲突检测、输入法屏蔽。
//! 从 `interact` 拆出，控制单文件规模；鼠标悬停与点击见 `super`。

use windows::Win32::UI::Input::Ime::{HIMC, ImmAssociateContext};
use windows::Win32::UI::Input::KeyboardAndMouse::GetKeyState;
use windows::Win32::UI::WindowsAndMessaging::{MB_ICONWARNING, MB_OK};

use crate::app::message_box;

use super::super::Panel;
use super::super::controls::{MOD_ALT_BIT, MOD_CTRL_BIT, MOD_SHIFT_BIT, MOD_WIN_BIT, RowId, conflict_for, spec};

impl Panel {
    /// 热键捕获键盘输入。
    pub(in crate::settings::panel) fn on_key(&mut self, vk: u32) {
        let Some(id) = self.capturing else {
            return;
        };
        match vk {
            0x1B => self.end_capture(), // Esc 取消
            // 纯修饰键：继续等待主键
            0x10 | 0x11 | 0x12 | 0x5B | 0x5C => {}
            _ => {
                // 鼠标键不参与修饰键匹配（ADR 0019）：不记录，避免显示成「Ctrl+侧键」却不生效
                let mods = if crate::app::mouse::is_mouse_key(vk) {
                    0
                } else {
                    held_modifiers()
                };
                // 与其它已配置键冲突则拒绝（M3）：热键看键+修饰，工具键按虚拟键匹配
                if let Some(name) = conflict_for(&self.config, id, vk, mods) {
                    message_box(
                        None,
                        &format!("按键 {name} 已被其他快捷键使用，请换一个。"),
                        MB_OK | MB_ICONWARNING,
                    );
                    self.end_capture();
                    return;
                }
                // 写回字段由元数据表决定（热键记键+修饰，覆盖层内键 / 工具键只记虚拟键）
                if let Some(key) = spec(id) {
                    key.apply(&mut self.config, vk, mods);
                }
                self.end_capture();
                self.config_dirty = true;
            }
        }
    }

    /// 开始捕获：禁用输入法（防止按键被 IME 改写为 0xE5 等键码）。
    pub(super) fn begin_capture(&mut self, id: RowId) {
        self.capturing = Some(id);
        unsafe {
            let old = ImmAssociateContext(self.hwnd, HIMC(std::ptr::null_mut()));
            if !old.0.is_null() {
                self.ime_old = old;
            }
        }
    }

    /// 结束捕获：恢复输入法。
    pub(in crate::settings::panel) fn end_capture(&mut self) {
        self.capturing = None;
        unsafe {
            if !self.ime_old.0.is_null() {
                let _ = ImmAssociateContext(self.hwnd, self.ime_old);
                self.ime_old = HIMC(std::ptr::null_mut());
            }
        }
    }

    /// 面板关闭兜底：恢复输入法（防止异常路径残留禁用态）。
    pub(in crate::settings::panel) fn restore_ime(&mut self) {
        unsafe {
            if !self.ime_old.0.is_null() {
                let _ = ImmAssociateContext(self.hwnd, self.ime_old);
                self.ime_old = HIMC(std::ptr::null_mut());
            }
        }
        self.capturing = None;
    }
}

/// 当前按住的修饰键位（与 `HotkeyConfig::modifiers` 的 MOD_* 位一致）。
fn held_modifiers() -> u32 {
    let mut m = 0u32;
    if key_down(0x10) {
        m |= MOD_SHIFT_BIT;
    }
    if key_down(0x11) {
        m |= MOD_CTRL_BIT;
    }
    if key_down(0x12) {
        m |= MOD_ALT_BIT;
    }
    if key_down(0x5B) || key_down(0x5C) {
        m |= MOD_WIN_BIT;
    }
    m
}

fn key_down(vk: i32) -> bool {
    unsafe { GetKeyState(vk) as u16 & 0x8000 != 0 }
}
