// SPDX-License-Identifier: GPL-3.0-only
//! 快捷键行元数据表（issue #140）：布局 / 键帽显示 / 冲突检测 / 捕获写回 / 清空都由这里
//! 的一张表驱动，替代原先散在 5 个文件的平行 match —— 那种写法漏改一处就会**静默出错**
//! （例如漏了 `row_key` → 该行清除按钮永远置灰），且 `pairs` 的数组长度还要手改。
//!
//! 新增一个键帽行只需两处：`RowId` 加变体、本表加一项（**并填上它的 `group`**，
//! 漏填会被 `every_key_row_belongs_to_exactly_one_group` 拦下）。
//! **表顺序即界面顺序**：同组内相邻排列，组与组之间由子页签分隔（ADR 0041），
//! 同时也决定冲突提示先报哪一项。

use crate::i18n::SettingsKey as K;
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
    /// 覆盖层内**带修饰键**的键（保存 / 快速保存，ADR 0041）：不注册全局热键，
    /// 但按键与修饰键一起写回、一起参与冲突比较（`Ctrl+S` 与 `Alt+S` 与无修饰的 `S` 各管一事）。
    OverlayCombo,
}

/// 快捷键页的子页签分组（ADR 0041）：20 行平铺太难扫，按「我现在想干什么」分成三组。
/// 组归元数据表（`KeyRowSpec::group`）唯一决定，布局 / 绘制 / 命中都从表里读。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum KeyGroup {
    /// 截图相关：截图、再次截图、全屏、上次区域。
    Capture,
    /// 输出相关：保存、快速保存、贴图、贴图边框、长截图。
    Output,
    /// 工具切换：移动 + 10 个标注工具。
    Tools,
}

impl KeyGroup {
    /// 页签顺序即界面顺序。
    pub const ALL: [KeyGroup; 3] = [KeyGroup::Capture, KeyGroup::Output, KeyGroup::Tools];

    pub fn title(self) -> &'static str {
        match self {
            KeyGroup::Capture => K::KeyTabCapture.text(),
            KeyGroup::Output => K::KeyTabOutput.text(),
            KeyGroup::Tools => K::KeyTabTools.text(),
        }
    }
}

/// 一个快捷键行的全部元数据。
pub struct KeyRowSpec {
    pub id: RowId,
    /// 行标签键，同时用作冲突提示名（「按键 {label} 已被其他快捷键使用」）——两者本就同字。
    pub label: K,
    /// 所属子页签（ADR 0041），决定它出现在哪一组里。
    pub group: KeyGroup,
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
        label: K,
        group: KeyGroup,
        confirm_clear: bool,
        get: fn(&HotkeyConfig, &ToolKeys) -> (u32, u32),
        set: fn(&mut Config, u32, u32),
    ) -> Self {
        Self {
            id,
            label,
            group,
            bind: KeyBind::Hotkey,
            get,
            set,
            confirm_clear,
        }
    }

    /// 覆盖层内键 / 工具切换键：只认虚拟键，无修饰键。
    const fn plain(
        id: RowId,
        label: K,
        group: KeyGroup,
        get: fn(&HotkeyConfig, &ToolKeys) -> (u32, u32),
        set: fn(&mut Config, u32, u32),
    ) -> Self {
        Self {
            id,
            label,
            group,
            bind: KeyBind::Plain,
            get,
            set,
            confirm_clear: false,
        }
    }

    /// 覆盖层内组合键（保存 / 快速保存，ADR 0041）：键 + 修饰键一起记录。
    const fn combo(
        id: RowId,
        label: K,
        group: KeyGroup,
        get: fn(&HotkeyConfig, &ToolKeys) -> (u32, u32),
        set: fn(&mut Config, u32, u32),
    ) -> Self {
        Self {
            id,
            label,
            group,
            bind: KeyBind::OverlayCombo,
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

mod table;
pub use table::KEY_ROWS;

/// 某一组的全部行（子页签只画自己那组）。
pub fn rows_of(group: KeyGroup) -> impl Iterator<Item = &'static KeyRowSpec> {
    KEY_ROWS.iter().filter(move |s| s.group == group)
}
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
                assert_ne!(a.label, b.label, "表里有两行的标签都是「{}」", a.label.text());
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
            let want_mods = if spec.bind == KeyBind::Plain { 0 } else { MOD_ALT_BIT };
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
                (s.label.text(), key_name(vk, mods))
            })
            .collect();
        let want: Vec<(&str, String)> = vec![
            // 顺序 = 元数据表顺序 = 各子页签内的界面顺序（ADR 0041）
            ("截图", "F1".into()),
            ("再次截图", "Shift+F1".into()),
            ("全屏", "F".into()),
            ("上次区域", "R".into()),
            ("保存", "Ctrl+S".into()),
            ("快速保存", "Alt+S".into()),
            ("贴图", "F3".into()),
            ("贴图边框", "Ctrl+F3".into()),
            ("长截图", "S".into()),
            ("移动", "V".into()),
            ("矩形", "1".into()),
            ("箭头", "2".into()),
            ("文本", "3".into()),
            ("模糊", "4".into()),
            ("荧光笔", "5".into()),
            ("聚光灯", "6".into()),
            ("序号", "7".into()),
            ("直线", "8".into()),
            ("椭圆", "9".into()),
            ("手绘", "0".into()),
        ];
        assert_eq!(got, want);
    }

    /// 冲突检测与写回共用同一张表（不会出现「检测算了 A、写回算成 B」）：
    /// 把任一项的键位填进另一项，必须报出第一个（按**实际写回形态**）真会撞上它的项。
    /// 注意跨类情形：目标是 `Plain` 行时修饰键会被丢弃，期望值也要按裸键算——
    /// 这正是一一对应关系不能写成「总是报 a 的标签」的原因。
    #[test]
    fn conflicts_cover_every_pair() {
        let cfg = Config::default();
        // 写回形态：`Plain` 行不记录修饰键
        let written_mods = |id: RowId, mods: u32| {
            if spec(id).is_some_and(|s| s.bind == KeyBind::Plain) {
                0
            } else {
                mods
            }
        };
        // 独立的期望值：扫全表取第一个撞车项（不复用 `conflict_for` 的实现）
        let expect = |skip: RowId, vk: u32, mods: u32| -> Option<&'static str> {
            KEY_ROWS
                .iter()
                .find(|c| {
                    if c.id == skip {
                        return false;
                    }
                    let (cv, cm) = c.current(&cfg);
                    if cv == crate::settings::UNBOUND {
                        return false;
                    }
                    match c.bind {
                        KeyBind::Hotkey | KeyBind::OverlayCombo => cv == vk && cm == mods,
                        KeyBind::Plain => cv == vk && mods == 0,
                    }
                })
                .map(|c| c.label.text())
        };
        for a in &KEY_ROWS {
            let (vk, mods) = a.current(&cfg);
            for b in &KEY_ROWS {
                if a.id == b.id {
                    continue;
                }
                let want = expect(b.id, vk, written_mods(b.id, mods));
                assert_eq!(
                    super::super::conflict_for(&cfg, b.id, vk, mods),
                    want,
                    "{:?} 占用 {:?} 的键位时应报 {want:?}",
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
