// SPDX-License-Identifier: GPL-3.0-only
//! 快捷键显示名与冲突检测（M3）：从 `controls` 拆出，保证单文件不超过 300 行。

use super::RowId;

/// 虚拟键 → 显示名（如 F1、Ctrl+C）。
/// 修饰位与 Windows `MOD_*` 一致：ALT=0x1、CONTROL=0x2、SHIFT=0x4、WIN=0x8。
pub fn key_name(vk: u32, mods: u32) -> String {
    let mut s = String::new();
    if mods & MOD_CTRL_BIT != 0 {
        s += "Ctrl+";
    }
    if mods & MOD_ALT_BIT != 0 {
        s += "Alt+";
    }
    if mods & MOD_SHIFT_BIT != 0 {
        s += "Shift+";
    }
    if mods & MOD_WIN_BIT != 0 {
        s += "Win+";
    }
    s += &vk_name(vk);
    s
}

/// 虚拟键 → 键名。
pub fn vk_name(vk: u32) -> String {
    match vk {
        0x70..=0x87 => format!("F{}", vk - 0x70 + 1),
        0x1B => "Esc".into(),
        0x20 => "Space".into(),
        0x09 => "Tab".into(),
        0x0D => "Enter".into(),
        0x2E => "Del".into(),
        0x27 => "→".into(),
        0x25 => "←".into(),
        0x26 => "↑".into(),
        0x28 => "↓".into(),
        // 常规字符键：MapVirtualKeyW 取键名（组合键 Shift+ 时字符可能大写/符号）
        _ => {
            use windows::Win32::UI::Input::KeyboardAndMouse::{MAPVK_VK_TO_CHAR, MapVirtualKeyW};
            unsafe {
                let ch = MapVirtualKeyW(vk, MAPVK_VK_TO_CHAR) & 0xFFFF;
                if ch != 0 && (ch as u8).is_ascii_graphic() {
                    (ch as u8 as char).to_uppercase().to_string()
                } else {
                    format!("键 {vk:#x}")
                }
            }
        }
    }
}

/// 修饰键位掩码（与 `HotkeyConfig::modifiers` 的 MOD_* 位一致）。
pub const MOD_ALT_BIT: u32 = 1 << 0;
pub const MOD_CTRL_BIT: u32 = 1 << 1;
pub const MOD_SHIFT_BIT: u32 = 1 << 2;
pub const MOD_WIN_BIT: u32 = 1 << 3;

/// 检查新键是否与其它已配置快捷键冲突（热键：键+修饰都相同；工具键：虚拟键相同即冲突）。
/// 返回冲突项名称；被检查项自身旧值不算冲突。
pub fn conflict_for(
    hk: &crate::settings::HotkeyConfig,
    tk: &crate::settings::ToolKeys,
    id: RowId,
    vk: u32,
    mods: u32,
) -> Option<&'static str> {
    // (当前项自值，用于跳过自身)
    let (self_vk, self_mods) = match id {
        RowId::HotkeyShot => (hk.screenshot_key, hk.screenshot_modifiers),
        RowId::HotkeyPin => (hk.pin_key, hk.pin_modifiers),
        RowId::HotkeyBorder => (hk.border_key, hk.border_modifiers),
        RowId::ToolRect => (tk.rect, 0),
        RowId::ToolArrow => (tk.arrow, 0),
        RowId::ToolText => (tk.text, 0),
        RowId::ToolBlur => (tk.blur, 0),
        RowId::ToolHighlight => (tk.highlight, 0),
        RowId::ToolGlow => (tk.glow, 0),
        _ => (u32::MAX, u32::MAX),
    };
    // 已配置键位：热键按「键+修饰」比较，工具键按「虚拟键」比较
    let pairs: [(&str, u32, u32, bool); 9] = [
        ("截图", hk.screenshot_key, hk.screenshot_modifiers, true),
        ("贴图", hk.pin_key, hk.pin_modifiers, true),
        ("贴图边框", hk.border_key, hk.border_modifiers, true),
        ("工具·矩形", tk.rect, 0, false),
        ("工具·箭头", tk.arrow, 0, false),
        ("工具·文本", tk.text, 0, false),
        ("工具·模糊", tk.blur, 0, false),
        ("工具·荧光笔", tk.highlight, 0, false),
        ("工具·聚光灯", tk.glow, 0, false),
    ];
    for (name, kv, km, is_hotkey) in pairs {
        if kv == self_vk && km == self_mods {
            continue; // 自身旧值
        }
        let hit = if is_hotkey {
            kv == vk && km == mods
        } else {
            // 工具键/跨类比较：按下即按虚拟键触发，故键相同即冲突
            kv == vk
        };
        if hit {
            return Some(name);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_names_are_sensible() {
        assert_eq!(vk_name(0x70), "F1");
        assert_eq!(key_name(0x43, MOD_CTRL_BIT), "Ctrl+C");
        assert_eq!(vk_name(0x1B), "Esc");
    }
}
