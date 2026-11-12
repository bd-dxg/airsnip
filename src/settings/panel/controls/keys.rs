// SPDX-License-Identifier: GPL-3.0-only
//! 快捷键显示的转出与冲突检测（M3）：显示名实现已移至 `settings::hotkey`（与键码同源，
//! 覆盖层悬浮提示 EDT-10 也要用），这里只做转出。从 `controls` 拆出，保证单文件不超 300 行。

use super::{KEY_ROWS, KeyBind, RowId, spec};

// 键名 / 修饰位掩码定义在 `settings::hotkey`（与键码同源），这里转出给设置面板与覆盖层用。
pub use crate::settings::hotkey::{MOD_ALT_BIT, MOD_CTRL_BIT, MOD_SHIFT_BIT, MOD_WIN_BIT, key_name};

/// 检查新键是否与其它已配置快捷键冲突（热键：键+修饰都相同；工具键：虚拟键相同即冲突）。
/// 返回冲突项标签；被检查项自身旧值不算冲突。
/// 逐项行为全部来自 `KEY_ROWS`（与布局、写回、清空同一张表），不另写平行 match。
pub fn conflict_for(cfg: &crate::settings::Config, id: RowId, vk: u32, mods: u32) -> Option<&'static str> {
    // 被检查项自身的旧值：跳过，否则「改成原来的键」会被自己拦下
    let self_pair = spec(id).map_or((u32::MAX, u32::MAX), |s| s.current(cfg));
    for row in &KEY_ROWS {
        let (kv, km) = row.current(cfg);
        if (kv, km) == self_pair {
            continue; // 自身旧值
        }
        if kv == crate::settings::UNBOUND {
            continue; // 已被清空的项不参与冲突（SYS-2）
        }
        let hit = if row.bind == KeyBind::Hotkey {
            kv == vk && km == mods
        } else {
            // 工具键/跨类比较：按下即按虚拟键触发，故键相同即冲突
            kv == vk
        };
        if hit {
            return Some(row.label);
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
        let mut cfg = crate::settings::Config::default();
        cfg.hotkey.screenshot_key = crate::settings::UNBOUND;
        assert_eq!(
            conflict_for(&cfg, RowId::HotkeyPin, 0x70, 0),
            None,
            "截图热键已清空，F1 应可分配给贴图"
        );
        // 反向：清空前的行为保持不变
        let cfg = crate::settings::Config::default();
        assert_eq!(
            conflict_for(&cfg, RowId::HotkeyPin, 0x70, 0),
            Some("截图"),
            "未清空时仍应报冲突"
        );
    }

    /// 长截图键：默认 S 不冲突；改成工具键或截图热键的键位应报冲突。
    #[test]
    fn longshot_key_conflicts() {
        let cfg = crate::settings::Config::default();
        assert_eq!(conflict_for(&cfg, RowId::HotkeyLongshot, 0x53, 0), None);
        assert_eq!(conflict_for(&cfg, RowId::HotkeyLongshot, 0x31, 0), Some("工具·矩形"));
        assert_eq!(conflict_for(&cfg, RowId::HotkeyLongshot, 0x70, 0), Some("截图"));
        // 反向：工具键改为 S 时也应被长截图键拦下
        assert_eq!(conflict_for(&cfg, RowId::ToolRect, 0x53, 0), Some("长截图"));
    }

    /// 再次截图键（issue #118）：默认 Shift+F1 与截图 F1 不冲突；改成截图键的同组合应报冲突。
    #[test]
    fn recapture_key_conflicts() {
        let cfg = crate::settings::Config::default();
        let (vk, mods) = (cfg.hotkey.recapture_key, cfg.hotkey.recapture_modifiers);
        assert_eq!(conflict_for(&cfg, RowId::HotkeyRecapture, vk, mods), None);
        // 与截图键同键同修饰、或与贴图键同键同修饰都应拦下
        assert_eq!(
            conflict_for(&cfg, RowId::HotkeyRecapture, cfg.hotkey.screenshot_key, 0),
            Some("截图")
        );
        assert_eq!(conflict_for(&cfg, RowId::HotkeyPin, vk, mods), Some("再次截图"));
    }
}
