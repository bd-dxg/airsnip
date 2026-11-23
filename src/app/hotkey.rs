// SPDX-License-Identifier: GPL-3.0-only
//! 全局热键（SYS-2）：注册、冲突提示、无修饰键风险首次提示；窗口内方向键步进。

use windows::Win32::Foundation::GetLastError;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    HOT_KEY_MODIFIERS, MOD_ALT, MOD_CONTROL, MOD_NOREPEAT, MOD_SHIFT, MOD_WIN, RegisterHotKey, UnregisterHotKey,
};
use windows::Win32::UI::WindowsAndMessaging::{MB_ICONINFORMATION, MB_ICONWARNING, MB_OK};

use super::{App, ID_HOTKEY_PIN, ID_HOTKEY_SHOT, message_box, mouse};

const ERROR_HOTKEY_ALREADY_REGISTERED: u32 = 1409;

impl App {
    /// 注册全局热键（SYS-2），失败时提示冲突。
    /// 先注销旧注册：改键后（M3 设置面板）id 可能先前已注册，直接 Register 会报冲突。
    pub fn register_hotkeys(&self) {
        unsafe {
            let _ = UnregisterHotKey(Some(self.hwnd), ID_HOTKEY_SHOT);
            let _ = UnregisterHotKey(Some(self.hwnd), ID_HOTKEY_PIN);
        }
        let hk = &self.config.hotkey;
        use crate::i18n::AppKey as A;
        self.register_one(
            ID_HOTKEY_SHOT,
            hk.screenshot_key,
            hk.screenshot_modifiers,
            A::HotkeyNameCapture.text(),
        );
        self.register_one(ID_HOTKEY_PIN, hk.pin_key, hk.pin_modifiers, A::HotkeyNamePin.text());
    }

    fn register_one(&self, id: i32, key: u32, modifiers: u32, name: &str) {
        // 已清空（`UNBOUND`）：调用方已 UnregisterHotKey，这里既不注册也不提示失败（SYS-2）
        if key == crate::settings::UNBOUND {
            return;
        }
        // 鼠标键不走 RegisterHotKey（不支持）：由 Raw Input 分发（ADR 0019）
        if crate::app::mouse::is_mouse_key(key) {
            return;
        }
        let mods = mod_flags(modifiers);
        let ok = unsafe { RegisterHotKey(Some(self.hwnd), id, mods, key) }.is_ok();
        if !ok {
            let err = unsafe { GetLastError() };
            use crate::i18n::{self, AppKey as A};
            let code = err.0.to_string();
            let text = if err.0 == ERROR_HOTKEY_ALREADY_REGISTERED {
                i18n::fill(A::HotkeyTaken.text(), &[("name", name)])
            } else {
                i18n::fill(A::HotkeyFailed.text(), &[("name", name), ("code", &code)])
            };
            message_box(Some(self.hwnd), &text, MB_OK | MB_ICONWARNING);
        }
    }

    /// SYS-2：无修饰键的全局热键会抢占其他程序的按键，仅首次启动提示一次。
    /// 已被清空的键不参与（否则把两个全局热键都关掉时还会提示没修饰键）。
    pub fn maybe_warn_no_modifier_clash(&mut self) {
        let hk = &self.config.hotkey;
        if !self.config.hotkey_warned
            && hk.screenshot_key != crate::settings::UNBOUND
            && hk.pin_key != crate::settings::UNBOUND
            && hk.screenshot_modifiers == 0
            && hk.pin_modifiers == 0
            // 鼠标键不抢占其他程序的键盘（ADR 0019）：两个都是鼠标键就不必警告
            && !(mouse::is_mouse_key(hk.screenshot_key) && mouse::is_mouse_key(hk.pin_key))
        {
            self.config.hotkey_warned = true;
            self.config.save();
            // 提示里的键名必须取**当前实际键位**：写死 F1 / F3 在用户改键后就是谎报。
            let shot = crate::settings::hotkey::key_name(hk.screenshot_key, 0);
            let pin = crate::settings::hotkey::key_name(hk.pin_key, 0);
            let text = crate::i18n::fill(
                crate::i18n::AppKey::HotkeyNoModifier.text(),
                &[("shot", &shot), ("pin", &pin)],
            );
            message_box(Some(self.hwnd), &text, MB_OK | MB_ICONINFORMATION);
        }
    }

    /// 全局热键分发（SYS-2）。
    pub fn handle_hotkey(&self, id: i32) {
        match id {
            ID_HOTKEY_SHOT => self.start_capture(),
            // PIN-2：非截图状态按 F3 → 剪贴板图片贴到屏幕（截图期间该热键已被撤销）
            ID_HOTKEY_PIN => {
                crate::pin::from_clipboard(self.hinstance, self.config.pin_position, self.config.pin_border)
            }
            _ => {}
        }
    }
}

/// 方向键微调步进（EDT-7 / CAP-2 / PIN-1）：1px，按住 Shift 5px。
/// 覆盖层（移动选中对象 / 选区框）与贴图窗口（移动窗口）共用。
pub(crate) fn arrow_step(vk: u32, shift: bool) -> Option<(i32, i32)> {
    let s = if shift { 5 } else { 1 };
    match vk {
        0x25 => Some((-s, 0)), // VK_LEFT
        0x26 => Some((0, -s)), // VK_UP
        0x27 => Some((s, 0)),  // VK_RIGHT
        0x28 => Some((0, s)),  // VK_DOWN
        _ => None,
    }
}

/// 组装热键修饰键位标志。
pub(crate) fn mod_flags(modifiers: u32) -> HOT_KEY_MODIFIERS {
    let mut m = HOT_KEY_MODIFIERS(0);
    if modifiers & MOD_ALT.0 != 0 {
        m |= MOD_ALT;
    }
    if modifiers & MOD_CONTROL.0 != 0 {
        m |= MOD_CONTROL;
    }
    if modifiers & MOD_SHIFT.0 != 0 {
        m |= MOD_SHIFT;
    }
    if modifiers & MOD_WIN.0 != 0 {
        m |= MOD_WIN;
    }
    m |= MOD_NOREPEAT;
    m
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 配置里的修饰位（`settings::MOD_*_BIT`）与 Win32 `MOD_*` 同值：映射后应一一对应，
    /// 且一律带 `MOD_NOREPEAT`（按住不连发）。再次截图键（issue #118）走同一套映射。
    #[test]
    fn mod_flags_map_config_bits() {
        use crate::settings::hotkey::{MOD_ALT_BIT, MOD_CTRL_BIT, MOD_SHIFT_BIT, MOD_WIN_BIT};
        let shift = mod_flags(MOD_SHIFT_BIT);
        assert_eq!(shift & MOD_SHIFT, MOD_SHIFT);
        assert_eq!(shift & MOD_CONTROL, HOT_KEY_MODIFIERS(0));
        assert_eq!(shift & MOD_NOREPEAT, MOD_NOREPEAT, "应带 MOD_NOREPEAT");
        let all = mod_flags(MOD_ALT_BIT | MOD_CTRL_BIT | MOD_SHIFT_BIT | MOD_WIN_BIT);
        for m in [MOD_ALT, MOD_CONTROL, MOD_SHIFT, MOD_WIN, MOD_NOREPEAT] {
            assert_eq!(all & m, m, "{m:?} 未被映射");
        }
        assert_eq!(mod_flags(0) & MOD_NOREPEAT, MOD_NOREPEAT, "无修饰键也应带 NOREPEAT");
    }

    #[test]
    fn arrow_step_uses_one_pixel_or_five_with_shift() {
        assert_eq!(arrow_step(0x25, false), Some((-1, 0)));
        assert_eq!(arrow_step(0x26, false), Some((0, -1)));
        assert_eq!(arrow_step(0x27, false), Some((1, 0)));
        assert_eq!(arrow_step(0x28, false), Some((0, 1)));
        assert_eq!(arrow_step(0x25, true), Some((-5, 0)));
        assert_eq!(arrow_step(0x28, true), Some((0, 5)));
        assert_eq!(arrow_step(0x41, false), None, "非方向键不产生步进");
    }
}
