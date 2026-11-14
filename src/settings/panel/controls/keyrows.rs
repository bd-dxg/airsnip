// SPDX-License-Identifier: GPL-3.0-only
//! 快捷键行元数据表（issue #140）：布局 / 键帽显示 / 冲突检测 / 捕获写回 / 清空都由这里
//! 的一张表驱动，替代原先散在 5 个文件的平行 match —— 那种写法漏改一处就会**静默出错**
//! （例如漏了 `row_key` → 该行清除按钮永远置灰），且 `pairs` 的数组长度还要手改。
//!
//! 新增一个键帽行只需两处：`RowId` 加变体、本表加一项。
//! **表顺序即界面顺序**（两列按此顺序从左到右、从上到下），同时也决定冲突提示先报哪一项。

use crate::settings::{Config, HotkeyConfig, ToolKeys};

use super::RowId;

/// 按键生效范围：决定冲突检测的比较粒度与写回是否记录修饰键。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum KeyBind {
    /// 全局热键（截图 / 再次截图 / 贴图 / 贴图边框）：比较「虚拟键 + 修饰键」，两者都写回。
    Hotkey,
    /// 覆盖层内按键（长截图）与工具切换键：按下即触发，只认虚拟键，修饰键恒为 0
    /// （捕获时按住的 Ctrl 不记录，比较时也不看）。
    Plain,
}

/// 一个快捷键行的全部元数据。
pub struct KeyRowSpec {
    pub id: RowId,
    /// 行标签，同时用作冲突提示名（「按键 {label} 已被其他快捷键使用」）——两者本就同字。
    pub label: &'static str,
    pub bind: KeyBind,
    /// 读当前键位 `(虚拟键, 修饰键)`；`Plain` 行的修饰键恒为 0。
    pub get: fn(&HotkeyConfig, &ToolKeys) -> (u32, u32),
    /// 写回键位；`Plain` 行忽略 `mods`。
    pub set: fn(&mut Config, u32, u32),
    /// 清除前二次确认：这两个热键是唯一入口，清空后只能从托盘菜单触发。
    pub confirm_clear: bool,
}

impl KeyRowSpec {
    /// 全局热键行：键 + 独立修饰键。`confirm_clear` 见字段说明。
    const fn hotkey(
        id: RowId,
        label: &'static str,
        confirm_clear: bool,
        get: fn(&HotkeyConfig, &ToolKeys) -> (u32, u32),
        set: fn(&mut Config, u32, u32),
    ) -> Self {
        Self {
            id,
            label,
            bind: KeyBind::Hotkey,
            get,
            set,
            confirm_clear,
        }
    }

    /// 覆盖层内键 / 工具切换键：只认虚拟键，无修饰键。
    const fn plain(
        id: RowId,
        label: &'static str,
        get: fn(&HotkeyConfig, &ToolKeys) -> (u32, u32),
        set: fn(&mut Config, u32, u32),
    ) -> Self {
        Self {
            id,
            label,
            bind: KeyBind::Plain,
            get,
            set,
            confirm_clear: false,
        }
    }

    /// 当前键位 `(虚拟键, 修饰键)`。
    pub fn current(&self, cfg: &Config) -> (u32, u32) {
        (self.get)(&cfg.hotkey, &cfg.tool_keys)
    }

    /// 写回键位。
    pub fn apply(&self, cfg: &mut Config, vk: u32, mods: u32) {
        (self.set)(cfg, vk, mods);
    }
}

/// 快捷键行元数据（顺序即界面顺序，见模块文档）。
pub const KEY_ROWS: [KeyRowSpec; 16] = [
    KeyRowSpec::hotkey(
        RowId::HotkeyShot,
        "截图",
        true,
        |hk, _| (hk.screenshot_key, hk.screenshot_modifiers),
        |cfg, vk, mods| {
            cfg.hotkey.screenshot_key = vk;
            cfg.hotkey.screenshot_modifiers = mods;
        },
    ),
    KeyRowSpec::hotkey(
        RowId::HotkeyRecapture,
        "再次截图",
        false,
        |hk, _| (hk.recapture_key, hk.recapture_modifiers),
        |cfg, vk, mods| {
            cfg.hotkey.recapture_key = vk;
            cfg.hotkey.recapture_modifiers = mods;
        },
    ),
    KeyRowSpec::hotkey(
        RowId::HotkeyPin,
        "贴图",
        true,
        |hk, _| (hk.pin_key, hk.pin_modifiers),
        |cfg, vk, mods| {
            cfg.hotkey.pin_key = vk;
            cfg.hotkey.pin_modifiers = mods;
        },
    ),
    KeyRowSpec::hotkey(
        RowId::HotkeyBorder,
        "贴图边框",
        false,
        |hk, _| (hk.border_key, hk.border_modifiers),
        |cfg, vk, mods| {
            cfg.hotkey.border_key = vk;
            cfg.hotkey.border_modifiers = mods;
        },
    ),
    // 长截图键在覆盖层内按下即触发，与工具键同组
    KeyRowSpec::plain(
        RowId::HotkeyLongshot,
        "长截图",
        |hk, _| (hk.longshot_key, 0),
        |cfg, vk, _| cfg.hotkey.longshot_key = vk,
    ),
    // ADR 0036：移动（指针）键排在工具键之前——它才是框选完成后的缺省态
    KeyRowSpec::plain(
        RowId::ToolMove,
        "工具·移动",
        |_, tk| (tk.move_key, 0),
        |cfg, vk, _| cfg.tool_keys.move_key = vk,
    ),
    KeyRowSpec::plain(
        RowId::ToolRect,
        "工具·矩形",
        |_, tk| (tk.rect, 0),
        |cfg, vk, _| cfg.tool_keys.rect = vk,
    ),
    KeyRowSpec::plain(
        RowId::ToolArrow,
        "工具·箭头",
        |_, tk| (tk.arrow, 0),
        |cfg, vk, _| cfg.tool_keys.arrow = vk,
    ),
    KeyRowSpec::plain(
        RowId::ToolText,
        "工具·文本",
        |_, tk| (tk.text, 0),
        |cfg, vk, _| cfg.tool_keys.text = vk,
    ),
    KeyRowSpec::plain(
        RowId::ToolBlur,
        "工具·模糊",
        |_, tk| (tk.blur, 0),
        |cfg, vk, _| cfg.tool_keys.blur = vk,
    ),
    KeyRowSpec::plain(
        RowId::ToolHighlight,
        "工具·荧光笔",
        |_, tk| (tk.highlight, 0),
        |cfg, vk, _| cfg.tool_keys.highlight = vk,
    ),
    KeyRowSpec::plain(
        RowId::ToolGlow,
        "工具·聚光灯",
        |_, tk| (tk.glow, 0),
        |cfg, vk, _| cfg.tool_keys.glow = vk,
    ),
    KeyRowSpec::plain(
        RowId::ToolNumber,
        "工具·序号",
        |_, tk| (tk.number, 0),
        |cfg, vk, _| cfg.tool_keys.number = vk,
    ),
    KeyRowSpec::plain(
        RowId::ToolLine,
        "工具·直线",
        |_, tk| (tk.line, 0),
        |cfg, vk, _| cfg.tool_keys.line = vk,
    ),
    KeyRowSpec::plain(
        RowId::ToolEllipse,
        "工具·椭圆",
        |_, tk| (tk.ellipse, 0),
        |cfg, vk, _| cfg.tool_keys.ellipse = vk,
    ),
    KeyRowSpec::plain(
        RowId::ToolFreehand,
        "工具·手绘",
        |_, tk| (tk.freehand, 0),
        |cfg, vk, _| cfg.tool_keys.freehand = vk,
    ),
];

/// 按控件 ID 取元数据；非快捷键行返回 `None`。
pub fn spec(id: RowId) -> Option<&'static KeyRowSpec> {
    KEY_ROWS.iter().find(|s| s.id == id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::hotkey::{MOD_ALT_BIT, key_name};
    use crate::settings::{Config, UNBOUND};

    /// 表内 ID 与标签都不许重复（复制一行改字段时最容易漏的就是这两个）。
    #[test]
    fn table_entries_are_unique() {
        for (i, a) in KEY_ROWS.iter().enumerate() {
            for b in &KEY_ROWS[i + 1..] {
                assert_ne!(a.id, b.id, "表里有两项都指向 {:?}", a.id);
                assert_ne!(a.label, b.label, "表里有两行的标签都是「{}」", a.label);
            }
        }
    }

    /// `get` / `set` 必须指向同一个字段：写进去什么就读出什么（`Plain` 行修饰键恒 0）。
    /// 这条专门抓「get 读 A 字段、set 写 B 字段」这类只能靠肉眼发现的静默错位。
    #[test]
    fn get_and_set_round_trip() {
        let vk = 0x7A; // F11，避开全部默认键位
        for spec in &KEY_ROWS {
            let mut cfg = Config::default();
            spec.apply(&mut cfg, vk, MOD_ALT_BIT);
            let want_mods = if spec.bind == KeyBind::Hotkey { MOD_ALT_BIT } else { 0 };
            assert_eq!(spec.current(&cfg), (vk, want_mods), "{:?} 写回后读不回原值", spec.id);
            // 清空也必须落在同一个字段上
            spec.apply(&mut cfg, UNBOUND, 0);
            assert_eq!(spec.current(&cfg), (UNBOUND, 0), "{:?} 清空后应读回未绑定", spec.id);
        }
    }

    /// 默认键位的显示文本：替代旧 `keycap_text` 的 16 分支 match 快照，
    /// 表项断错字段、标签写错这里就红。
    #[test]
    fn default_key_caps_text() {
        let cfg = Config::default();
        let got: Vec<(&str, String)> = KEY_ROWS
            .iter()
            .map(|s| {
                let (vk, mods) = s.current(&cfg);
                (s.label, key_name(vk, mods))
            })
            .collect();
        let want: Vec<(&str, String)> = vec![
            ("截图", "F1".into()),
            ("再次截图", "Shift+F1".into()),
            ("贴图", "F3".into()),
            ("贴图边框", "Ctrl+F3".into()),
            ("长截图", "S".into()),
            ("工具·移动", "V".into()),
            ("工具·矩形", "1".into()),
            ("工具·箭头", "2".into()),
            ("工具·文本", "3".into()),
            ("工具·模糊", "4".into()),
            ("工具·荧光笔", "5".into()),
            ("工具·聚光灯", "6".into()),
            ("工具·序号", "7".into()),
            ("工具·直线", "8".into()),
            ("工具·椭圆", "9".into()),
            ("工具·手绘", "0".into()),
        ];
        assert_eq!(got, want);
    }

    /// 冲突检测与写回共用同一张表（不会出现「检测算了 A、写回算成 B」）：
    /// 把任一项的键位填进另一项，必须报出前者的标签。
    #[test]
    fn conflicts_cover_every_pair() {
        let cfg = Config::default();
        for a in &KEY_ROWS {
            let (vk, mods) = a.current(&cfg);
            for b in &KEY_ROWS {
                if a.id == b.id {
                    continue;
                }
                assert_eq!(
                    super::super::conflict_for(&cfg, b.id, vk, mods),
                    Some(a.label),
                    "{:?} 占用 {:?} 的键位时应报冲突",
                    a.id,
                    b.id
                );
            }
        }
    }

    /// 非快捷键行既不在表里，也不参与冲突检测。
    #[test]
    fn non_key_rows_have_no_spec() {
        let cfg = Config::default();
        for id in [RowId::ResetKeys, RowId::Widenote, RowId::Autostart] {
            assert!(spec(id).is_none(), "{id:?} 不是快捷键行");
            assert_eq!(super::super::conflict_for(&cfg, id, 0x7A, 0), None, "{id:?} 不参与冲突");
        }
    }
}
