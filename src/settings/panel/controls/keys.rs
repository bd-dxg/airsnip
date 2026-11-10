// SPDX-License-Identifier: GPL-3.0-only
//! 快捷键显示的转出与冲突检测（M3）：显示名实现已移至 `settings::hotkey`（与键码同源，
//! 覆盖层悬浮提示 EDT-10 也要用），这里只做转出。从 `controls` 拆出，保证单文件不超 300 行。

use super::RowId;

// 键名 / 修饰位掩码定义在 `settings::hotkey`（与键码同源），这里转出给设置面板与覆盖层用。
pub use crate::settings::hotkey::{MOD_ALT_BIT, MOD_CTRL_BIT, MOD_SHIFT_BIT, MOD_WIN_BIT, key_name};

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
        RowId::HotkeyLongshot => (hk.longshot_key, 0),
        RowId::HotkeyOcr => (hk.ocr_key, 0),
        RowId::ToolRect => (tk.rect, 0),
        RowId::ToolArrow => (tk.arrow, 0),
        RowId::ToolText => (tk.text, 0),
        RowId::ToolBlur => (tk.blur, 0),
        RowId::ToolHighlight => (tk.highlight, 0),
        RowId::ToolGlow => (tk.glow, 0),
        RowId::ToolNumber => (tk.number, 0),
        RowId::ToolLine => (tk.line, 0),
        RowId::ToolEllipse => (tk.ellipse, 0),
        RowId::ToolFreehand => (tk.freehand, 0),
        _ => (u32::MAX, u32::MAX),
    };
    // 已配置键位：热键按「键+修饰」比较，工具键按「虚拟键」比较
    let pairs: [(&str, u32, u32, bool); 15] = [
        ("截图", hk.screenshot_key, hk.screenshot_modifiers, true),
        ("贴图", hk.pin_key, hk.pin_modifiers, true),
        ("贴图边框", hk.border_key, hk.border_modifiers, true),
        // 长截图键在覆盖层内无修饰键，与工具键同组
        ("长截图", hk.longshot_key, 0, false),
        ("OCR", hk.ocr_key, 0, false),
        ("工具·矩形", tk.rect, 0, false),
        ("工具·箭头", tk.arrow, 0, false),
        ("工具·文本", tk.text, 0, false),
        ("工具·模糊", tk.blur, 0, false),
        ("工具·荧光笔", tk.highlight, 0, false),
        ("工具·聚光灯", tk.glow, 0, false),
        ("工具·序号", tk.number, 0, false),
        ("工具·直线", tk.line, 0, false),
        ("工具·椭圆", tk.ellipse, 0, false),
        ("工具·手绘", tk.freehand, 0, false),
    ];
    for (name, kv, km, is_hotkey) in pairs {
        if kv == self_vk && km == self_mods {
            continue; // 自身旧值
        }
        if kv == crate::settings::UNBOUND {
            continue; // 已被清空的项不参与冲突（SYS-2）
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
    use crate::settings::hotkey::vk_name;

    #[test]
    fn key_names_are_sensible() {
        assert_eq!(vk_name(0x70), "F1");
        assert_eq!(key_name(0x43, MOD_CTRL_BIT), "Ctrl+C");
        assert_eq!(vk_name(0x1B), "Esc");
        assert_eq!(vk_name(0x2C), "PrtScn", "PrintScreen 应有可读键名（#67）");
        // 无字符的功能键走系统键名，不应退化成「键 0x13」（#67）
        assert!(!vk_name(0x13).contains("0x"), "Pause 应有可读键名：{}", vk_name(0x13));
        assert_eq!(vk_name(0x05), "鼠标侧键1", "鼠标键应有可读键名（ADR 0019）");
        assert_eq!(key_name(0, 0), "未设置", "未绑定的键不应显示为「键 0x0」");
    }

    /// 清空后的项不再参与冲突检测（否则清空 A 后把 B 设为 A 原来的键会误判）。
    #[test]
    fn unbound_keys_skip_conflict_check() {
        let mut hk = crate::settings::HotkeyConfig::default();
        let tk = crate::settings::ToolKeys::default();
        hk.screenshot_key = crate::settings::UNBOUND;
        assert_eq!(
            conflict_for(&hk, &tk, RowId::HotkeyPin, 0x70, 0),
            None,
            "截图热键已清空，F1 应可分配给贴图"
        );
        // 反向：清空前的行为保持不变
        let hk = crate::settings::HotkeyConfig::default();
        assert_eq!(
            conflict_for(&hk, &tk, RowId::HotkeyPin, 0x70, 0),
            Some("截图"),
            "未清空时仍应报冲突"
        );
    }

    /// 长截图键：默认 S 不冲突；改成工具键或截图热键的键位应报冲突。
    #[test]
    fn longshot_key_conflicts() {
        let hk = crate::settings::HotkeyConfig::default();
        let tk = crate::settings::ToolKeys::default();
        assert_eq!(conflict_for(&hk, &tk, RowId::HotkeyLongshot, 0x53, 0), None);
        assert_eq!(
            conflict_for(&hk, &tk, RowId::HotkeyLongshot, 0x31, 0),
            Some("工具·矩形")
        );
        assert_eq!(conflict_for(&hk, &tk, RowId::HotkeyLongshot, 0x70, 0), Some("截图"));
        // 反向：工具键改为 S 时也应被长截图键拦下
        assert_eq!(conflict_for(&hk, &tk, RowId::ToolRect, 0x53, 0), Some("长截图"));
    }

    /// OCR 键（OCR-3）：默认 T 不冲突；与长截图 / 工具键同键位应报冲突。
    #[test]
    fn ocr_key_conflicts() {
        let hk = crate::settings::HotkeyConfig::default();
        let tk = crate::settings::ToolKeys::default();
        assert_eq!(conflict_for(&hk, &tk, RowId::HotkeyOcr, 0x54, 0), None);
        assert_eq!(conflict_for(&hk, &tk, RowId::HotkeyOcr, 0x53, 0), Some("长截图"));
        assert_eq!(conflict_for(&hk, &tk, RowId::HotkeyOcr, 0x31, 0), Some("工具·矩形"));
        // 反向：把长截图键改为 T 时也应被 OCR 键拦下
        assert_eq!(conflict_for(&hk, &tk, RowId::HotkeyLongshot, 0x54, 0), Some("OCR"));
    }
}
