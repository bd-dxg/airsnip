// SPDX-License-Identifier: GPL-3.0-only
//! 快捷键显示的转出与冲突检测（M3）：显示名实现已移至 `settings::hotkey`（与键码同源，
//! 覆盖层悬浮提示 EDT-10 也要用），这里只做转出。从 `controls` 拆出，保证单文件不超 300 行。

use super::{KEY_ROWS, KeyBind, RowId, spec};

// 键名 / 修饰位掩码定义在 `settings::hotkey`（与键码同源），这里转出给设置面板与覆盖层用。
pub use crate::settings::hotkey::{MOD_ALT_BIT, MOD_CTRL_BIT, MOD_SHIFT_BIT, MOD_WIN_BIT, key_name};

/// 检查新键是否与其它已配置快捷键冲突（热键 / 组合键：键+修饰都相同；无修饰键的行：虚拟键相同
/// 且候选未带修饰键即冲突）。返回冲突项标签；被检查项自身旧值不算冲突。
/// 逐项行为全部来自 `KEY_ROWS`（与布局、写回、清空同一张表），不另写平行 match。
pub fn conflict_for(cfg: &crate::settings::Config, id: RowId, vk: u32, mods: u32) -> Option<&'static str> {
    // 被检查项自身的旧值：跳过，否则「改成原来的键」会被自己拦下
    let self_pair = spec(id).map_or((u32::MAX, u32::MAX), |s| s.current(cfg));
    // **目标是哪一类行**决定这次捕获最终会写成什么：`Plain` 行的 `set` 会丢掉修饰键，
    // 所以必须按裸虚拟键去比——否则「按住 Ctrl 捕获工具键」会绕过冲突检测，
    // 写回后得到一个与别的 Plain 行（或全局热键）静默重名的裸键。
    let cand_mods = if spec(id).is_some_and(|s| s.bind == KeyBind::Plain) {
        0
    } else {
        mods
    };
    for row in &KEY_ROWS {
        let (kv, km) = row.current(cfg);
        if (kv, km) == self_pair {
            continue; // 自身旧值
        }
        if kv == crate::settings::UNBOUND {
            continue; // 已被清空的项不参与冲突（SYS-2）
        }
        let hit = match row.bind {
            // 全局热键与覆盖层内组合键（ADR 0041）：键 + 修饰键都相同才算撞车
            KeyBind::Hotkey | KeyBind::OverlayCombo => kv == vk && km == cand_mods,
            // 无修饰键的覆盖层内键（长截图 / 工具键）：按下即按虚拟键触发，
            // 因此只与「不带修饰键」的候选撞车——`Ctrl+S` / `Alt+S` 与 `S` 天然共存。
            KeyBind::Plain => kv == vk && cand_mods == 0,
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

    /// 快速保存 / 保存（ADR 0041）：同键 S 不同修饰键互不冲突，与无修饰的长截图键也能共存；
    /// 把组合键拆成无修饰的裸键才会撞上长截图。
    #[test]
    fn save_combo_keys_coexist_with_plain_keys() {
        use crate::settings::hotkey::{MOD_ALT_BIT, MOD_CTRL_BIT};
        let cfg = crate::settings::Config::default();
        // 默认配置下两个组合键都不冲突
        assert_eq!(conflict_for(&cfg, RowId::HotkeySave, 0x53, MOD_CTRL_BIT), None);
        assert_eq!(conflict_for(&cfg, RowId::HotkeyQuickSave, 0x53, MOD_ALT_BIT), None);
        // 快速保存改成无修饰的 S → 撞长截图
        assert_eq!(conflict_for(&cfg, RowId::HotkeyQuickSave, 0x53, 0), Some("长截图"));
        // 两者互换成对方的组合 → 互相拦下
        assert_eq!(
            conflict_for(&cfg, RowId::HotkeyQuickSave, 0x53, MOD_CTRL_BIT),
            Some("保存")
        );
        // 长截图是无修饰行，捕获时按住的 Alt 会被 `set` 丢掉，最终写入的就是裸 S（= 原值），
        // 因此这里不该报「撞快速保存」——报冲突反而误导用户。
        assert_eq!(conflict_for(&cfg, RowId::HotkeyLongshot, 0x53, MOD_ALT_BIT), None);
    }

    /// 回归：目标是 `Plain` 行时，**按住的修饰键必须被忽略**再比一次。
    /// 否则「按住 Ctrl 捕获工具键」会绕过冲突检测，写回后与其它 Plain 行静默重名
    /// （用户看到面板显示 `2`，按下去却不知道触发的是哪个工具）。
    #[test]
    fn plain_target_ignores_held_modifiers() {
        use crate::settings::hotkey::{MOD_ALT_BIT, MOD_CTRL_BIT, MOD_SHIFT_BIT};
        let cfg = crate::settings::Config::default();
        // 矩形绑到箭头的键：按住任意修饰键都必须照样拦下
        for mods in [0, MOD_CTRL_BIT, MOD_SHIFT_BIT, MOD_ALT_BIT] {
            assert_eq!(
                conflict_for(&cfg, RowId::ToolRect, 0x32, mods),
                Some("箭头"),
                "mods={mods:#x} 时应拦下"
            );
        }
        // 工具键绑到无修饰全局热键的键（F1）：也要拦
        assert_eq!(conflict_for(&cfg, RowId::ToolRect, 0x70, MOD_CTRL_BIT), Some("截图"));
        // 反过来：目标带修饰键时按 (vk, mods) 比，裸键候选不算撞车
        assert_eq!(conflict_for(&cfg, RowId::HotkeySave, 0x31, 0), Some("矩形"));
    }

    /// 长截图键：默认 S 不冲突；改成工具键或截图热键的键位应报冲突。
    #[test]
    fn longshot_key_conflicts() {
        let cfg = crate::settings::Config::default();
        assert_eq!(conflict_for(&cfg, RowId::HotkeyLongshot, 0x53, 0), None);
        assert_eq!(conflict_for(&cfg, RowId::HotkeyLongshot, 0x31, 0), Some("矩形"));
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
