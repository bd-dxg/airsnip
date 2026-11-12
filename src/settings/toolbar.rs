// SPDX-License-Identifier: GPL-3.0-only
//! 工具栏可见性（EDT-16，issue #143）：把工具栏收成用户自己常用的那一小块。
//!
//! 这是**纯视觉**偏好，不是功能开关：隐藏不改变任何能力 —— 快捷键照旧生效、
//! 输出行为照旧、标注照旧用上次的颜色。关键词是「隐藏 ≠ 禁用」（见 ADR 0033）。
//!
//! 因此配置只存**隐藏集合**且用稳定字符串键（`[toolbar] hidden = ["tool.ellipse"]`）：
//! - 默认全可见，用户不动设置时界面与没有这个功能时完全一致；
//! - 以后新增控件不进任何人的隐藏集合，就自动可见；
//! - 不用枚举序号，工具重排也不会串位。

use serde::{Deserialize, Serialize};

use crate::editor::{SWATCH_COUNT, Tool};

/// 工具栏分组（设置面板「工具栏」页的组头，顺序即界面顺序）。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ToolbarGroup {
    Tools,
    Colors,
    Edit,
    Output,
    File,
}

const TOOL_SLOTS: [ToolbarSlot; 10] = [
    ToolbarSlot::Tool(Tool::Rect),
    ToolbarSlot::Tool(Tool::Arrow),
    ToolbarSlot::Tool(Tool::Text),
    ToolbarSlot::Tool(Tool::Highlight),
    ToolbarSlot::Tool(Tool::Blur),
    ToolbarSlot::Tool(Tool::Glow),
    ToolbarSlot::Tool(Tool::Number),
    ToolbarSlot::Tool(Tool::Line),
    ToolbarSlot::Tool(Tool::Ellipse),
    ToolbarSlot::Tool(Tool::Freehand),
];
const COLOR_SLOTS: [ToolbarSlot; SWATCH_COUNT] = [
    ToolbarSlot::Color(0),
    ToolbarSlot::Color(1),
    ToolbarSlot::Color(2),
    ToolbarSlot::Color(3),
    ToolbarSlot::Color(4),
];
const EDIT_SLOTS: [ToolbarSlot; 4] = [
    ToolbarSlot::Fill,
    ToolbarSlot::Undo,
    ToolbarSlot::Redo,
    ToolbarSlot::TextRow,
];
const OUTPUT_SLOTS: [ToolbarSlot; 2] = [ToolbarSlot::Pin, ToolbarSlot::Longshot];
const FILE_SLOTS: [ToolbarSlot; 2] = [ToolbarSlot::Copy, ToolbarSlot::Save];

impl ToolbarGroup {
    pub const ALL: [ToolbarGroup; 5] = [
        ToolbarGroup::Tools,
        ToolbarGroup::Colors,
        ToolbarGroup::Edit,
        ToolbarGroup::Output,
        ToolbarGroup::File,
    ];

    /// 组头文案。
    pub fn title(self) -> &'static str {
        match self {
            ToolbarGroup::Tools => "工具",
            ToolbarGroup::Colors => "颜色",
            ToolbarGroup::Edit => "编辑",
            ToolbarGroup::Output => "输出",
            ToolbarGroup::File => "文件",
        }
    }

    /// 组内元素（顺序即工具栏顺序）。
    pub fn slots(self) -> &'static [ToolbarSlot] {
        match self {
            ToolbarGroup::Tools => &TOOL_SLOTS,
            ToolbarGroup::Colors => &COLOR_SLOTS,
            ToolbarGroup::Edit => &EDIT_SLOTS,
            ToolbarGroup::Output => &OUTPUT_SLOTS,
            ToolbarGroup::File => &FILE_SLOTS,
        }
    }

    /// 设置面板每行排几个（多列紧凑排布，面板没有滚动条）。
    /// 统一 5 列：单元格宽度相同，「标签 + 开关」贴得够近，不会看错归属；
    /// 标签长度受此约束（实测字宽见 `controls` 的 `toolbar_cells_fit_label_and_toggle`）。
    pub fn columns(self) -> usize {
        5
    }
}

/// 工具栏里可单独隐藏的元素。
///
/// 文本第二行是**整行一个元素**（字体族 / B / I / 字号一起显隐），不拆逐元素 ——
/// 只留「B」会得到一个 26px 宽的第二行。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ToolbarSlot {
    Tool(Tool),
    Color(usize),
    /// 填充开关（EDT-1/EDT-14）。
    Fill,
    Undo,
    Redo,
    /// 文本第二行（EDT-4，字体族 / 粗体 / 斜体 / 字号）。
    TextRow,
    Pin,
    Longshot,
    Copy,
    Save,
}

impl ToolbarSlot {
    /// 全部元素（组顺序在前的排在前）：按组拼接，避免两处各写一份清单。
    pub fn all() -> impl Iterator<Item = ToolbarSlot> {
        ToolbarGroup::ALL.into_iter().flat_map(|g| g.slots().iter().copied())
    }

    /// 配置里的稳定键。**改这个等于改配置格式**（老用户的隐藏项会失效、退回可见）。
    pub fn key(self) -> &'static str {
        match self {
            ToolbarSlot::Tool(Tool::Rect) => "tool.rect",
            ToolbarSlot::Tool(Tool::Arrow) => "tool.arrow",
            ToolbarSlot::Tool(Tool::Text) => "tool.text",
            ToolbarSlot::Tool(Tool::Highlight) => "tool.highlight",
            ToolbarSlot::Tool(Tool::Blur) => "tool.blur",
            ToolbarSlot::Tool(Tool::Glow) => "tool.glow",
            ToolbarSlot::Tool(Tool::Number) => "tool.number",
            ToolbarSlot::Tool(Tool::Line) => "tool.line",
            ToolbarSlot::Tool(Tool::Ellipse) => "tool.ellipse",
            ToolbarSlot::Tool(Tool::Freehand) => "tool.freehand",
            ToolbarSlot::Color(0) => "color.1",
            ToolbarSlot::Color(1) => "color.2",
            ToolbarSlot::Color(2) => "color.3",
            ToolbarSlot::Color(3) => "color.4",
            ToolbarSlot::Color(4) => "color.5",
            // 色块数量变了也不该 panic：多余的落一个不会命中的键（默认可见）
            ToolbarSlot::Color(_) => "color.unused",
            ToolbarSlot::Fill => "edit.fill",
            ToolbarSlot::Undo => "edit.undo",
            ToolbarSlot::Redo => "edit.redo",
            ToolbarSlot::TextRow => "edit.text_row",
            ToolbarSlot::Pin => "output.pin",
            ToolbarSlot::Longshot => "output.longshot",
            ToolbarSlot::Copy => "file.copy",
            ToolbarSlot::Save => "file.save",
        }
    }

    /// 设置面板行标签（短，工具栏页是多列布局）。
    pub fn label(self) -> &'static str {
        match self {
            ToolbarSlot::Tool(Tool::Rect) => "矩形",
            ToolbarSlot::Tool(Tool::Arrow) => "箭头",
            ToolbarSlot::Tool(Tool::Text) => "文本",
            ToolbarSlot::Tool(Tool::Highlight) => "荧光笔",
            ToolbarSlot::Tool(Tool::Blur) => "模糊",
            ToolbarSlot::Tool(Tool::Glow) => "聚光灯",
            ToolbarSlot::Tool(Tool::Number) => "序号",
            ToolbarSlot::Tool(Tool::Line) => "直线",
            ToolbarSlot::Tool(Tool::Ellipse) => "椭圆",
            ToolbarSlot::Tool(Tool::Freehand) => "手绘",
            ToolbarSlot::Color(0) => "颜色1",
            ToolbarSlot::Color(1) => "颜色2",
            ToolbarSlot::Color(2) => "颜色3",
            ToolbarSlot::Color(3) => "颜色4",
            ToolbarSlot::Color(4) => "颜色5",
            ToolbarSlot::Color(_) => "颜色",
            ToolbarSlot::Fill => "填充",
            ToolbarSlot::Undo => "撤销",
            ToolbarSlot::Redo => "重做",
            // 第二行整行（字体族 / B / I / 字号）：多列布局里只能取短名，组头「编辑」提供上下文
            ToolbarSlot::TextRow => "字体行",
            ToolbarSlot::Pin => "贴图",
            ToolbarSlot::Longshot => "长截图",
            ToolbarSlot::Copy => "复制",
            ToolbarSlot::Save => "保存",
        }
    }

    /// 所属分组。
    pub fn group(self) -> ToolbarGroup {
        match self {
            ToolbarSlot::Tool(_) => ToolbarGroup::Tools,
            ToolbarSlot::Color(_) => ToolbarGroup::Colors,
            ToolbarSlot::Fill | ToolbarSlot::Undo | ToolbarSlot::Redo | ToolbarSlot::TextRow => ToolbarGroup::Edit,
            ToolbarSlot::Pin | ToolbarSlot::Longshot => ToolbarGroup::Output,
            ToolbarSlot::Copy | ToolbarSlot::Save => ToolbarGroup::File,
        }
    }
}

/// 一组元素的整体可见状态（组头的三态勾选框）。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum GroupState {
    /// 全可见。
    All,
    /// 部分可见（半选）。
    Partial,
    /// 全隐藏。
    None,
}

/// 工具栏可见性配置：只存隐藏集合，默认全可见（EDT-16）。
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ToolbarConfig {
    /// 隐藏的元素键（`ToolbarSlot::key`）；空数组 = 全部可见。
    pub hidden: Vec<String>,
}

impl ToolbarConfig {
    /// 单个元素是否绘制。
    pub fn visible(&self, slot: ToolbarSlot) -> bool {
        !self.hidden.iter().any(|k| k == slot.key())
    }

    /// 改单个元素显隐（幂等；重复键不会堆积）。
    pub fn set_visible(&mut self, slot: ToolbarSlot, on: bool) {
        let key = slot.key();
        if on {
            self.hidden.retain(|k| k != key);
        } else if self.visible(slot) {
            self.hidden.push(key.to_string());
        }
    }

    /// 组内可见元素数。
    pub fn visible_count(&self, group: ToolbarGroup) -> usize {
        group.slots().iter().filter(|s| self.visible(**s)).count()
    }

    /// 组的整体状态（组头勾选框）。
    pub fn group_state(&self, group: ToolbarGroup) -> GroupState {
        match self.visible_count(group) {
            0 => GroupState::None,
            n if n == group.slots().len() => GroupState::All,
            _ => GroupState::Partial,
        }
    }

    /// 整组全显 / 全隐。
    pub fn set_group_visible(&mut self, group: ToolbarGroup, on: bool) {
        for slot in group.slots() {
            self.set_visible(*slot, on);
        }
    }

    /// 第一个可见工具（开局回落用）；全隐藏时为 `None`。
    pub fn first_visible_tool(&self) -> Option<Tool> {
        Tool::ALL.into_iter().find(|t| self.visible(ToolbarSlot::Tool(*t)))
    }

    /// 第一个可见色块（开局回落用）；全隐藏时为 `None`。
    pub fn first_visible_color(&self) -> Option<usize> {
        (0..SWATCH_COUNT).find(|i| self.visible(ToolbarSlot::Color(*i)))
    }

    /// 开局回落（R5）：要用的工具被隐藏时落到第一个可见工具；
    /// 一个工具都不可见时只好保留原值（会话内快捷键仍可切过去）。
    pub fn settle_tool(&self, want: Tool) -> Tool {
        if self.visible(ToolbarSlot::Tool(want)) {
            return want;
        }
        self.first_visible_tool().unwrap_or(want)
    }

    /// 开局回落（R5）：当前色块被隐藏时落到第一个可见色块；
    /// 一个色块都不可见时保留原颜色（标注照常上色）。
    pub fn settle_color(&self, want: usize) -> usize {
        if self.visible(ToolbarSlot::Color(want)) {
            return want;
        }
        self.first_visible_color().unwrap_or(want)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::Config;

    /// `all()` 必须等于各组按组顺序拼接（漏写一项就意味着有一项用户永远改不了）。
    #[test]
    fn all_covers_every_group_slot() {
        let mut joined: Vec<ToolbarSlot> = Vec::new();
        for g in ToolbarGroup::ALL {
            joined.extend_from_slice(g.slots());
        }
        assert_eq!(
            joined,
            ToolbarSlot::all().collect::<Vec<_>>(),
            "all() 与各组的元素集合不一致"
        );
    }

    /// 键与标签都不许重复，且分组归属与所在组一致。
    #[test]
    fn keys_labels_and_groups_are_consistent() {
        let all: Vec<ToolbarSlot> = ToolbarSlot::all().collect();
        for (i, a) in all.iter().enumerate() {
            assert!(!a.key().is_empty());
            assert!(!a.label().is_empty());
            for b in &all[i + 1..] {
                assert_ne!(a.key(), b.key(), "键重复：{}", a.key());
                assert_ne!(a.label(), b.label(), "标签重复：{}", a.label());
            }
            let g = a.group();
            assert!(g.slots().contains(a), "{:?} 不在自己声明的组 {:?} 里", a, g);
        }
    }

    /// 键是配置格式的一部分：改名等于让老配置的隐藏项失效，这里钉死几个代表。
    #[test]
    fn keys_are_stable() {
        assert_eq!(ToolbarSlot::Tool(Tool::Rect).key(), "tool.rect");
        assert_eq!(ToolbarSlot::Color(0).key(), "color.1");
        assert_eq!(ToolbarSlot::TextRow.key(), "edit.text_row");
        assert_eq!(ToolbarSlot::Longshot.key(), "output.longshot");
        assert_eq!(ToolbarSlot::Save.key(), "file.save");
    }

    /// 默认全可见（用户不动设置时界面与没有这个功能时一致）。
    #[test]
    fn default_is_all_visible() {
        let cfg = ToolbarConfig::default();
        for g in ToolbarGroup::ALL {
            assert_eq!(cfg.group_state(g), GroupState::All, "{:?} 默认应全可见", g);
        }
    }

    /// 单项显隐幂等，且按组统计三态正确。
    #[test]
    fn single_toggle_and_group_state() {
        let mut cfg = ToolbarConfig::default();
        cfg.set_visible(ToolbarSlot::Pin, false);
        cfg.set_visible(ToolbarSlot::Pin, false);
        assert_eq!(cfg.hidden.len(), 1, "重复隐藏不应堆积重复键");
        assert!(!cfg.visible(ToolbarSlot::Pin));
        assert_eq!(cfg.group_state(ToolbarGroup::Output), GroupState::Partial);

        cfg.set_visible(ToolbarSlot::Pin, true);
        cfg.set_visible(ToolbarSlot::Pin, true);
        assert!(cfg.hidden.is_empty(), "全显后不应残留键");
        assert_eq!(cfg.group_state(ToolbarGroup::Output), GroupState::All);
    }

    /// 组头批量：部分可见时「全隐」，全隐时「全显」，全显时「全隐」。
    #[test]
    fn group_batch_covers_every_slot() {
        let mut cfg = ToolbarConfig::default();
        for g in ToolbarGroup::ALL {
            cfg.set_group_visible(g, false);
            assert_eq!(cfg.group_state(g), GroupState::None, "{:?} 应全隐", g);
            cfg.set_group_visible(g, true);
            assert_eq!(cfg.group_state(g), GroupState::All, "{:?} 应全显", g);
        }
        assert!(cfg.hidden.is_empty(), "全显后隐藏集合应为空：{:?}", cfg.hidden);
    }

    /// 开局回落（R5）：隐藏的工具 / 色块落到第一个可见项，全隐藏时保留原值。
    #[test]
    fn settle_falls_back_to_first_visible() {
        let mut cfg = ToolbarConfig::default();
        assert_eq!(cfg.settle_tool(Tool::Ellipse), Tool::Ellipse, "可见时不应改动");
        cfg.set_visible(ToolbarSlot::Tool(Tool::Rect), false);
        cfg.set_visible(ToolbarSlot::Tool(Tool::Arrow), false);
        assert_eq!(cfg.settle_tool(Tool::Rect), Tool::Text, "应落到第一个可见工具");

        cfg.set_group_visible(ToolbarGroup::Tools, false);
        assert_eq!(cfg.settle_tool(Tool::Rect), Tool::Rect, "一个工具都不可见时保留原值");
        assert_eq!(cfg.first_visible_tool(), None);

        cfg.set_visible(ToolbarSlot::Color(1), false);
        assert_eq!(cfg.settle_color(1), 0, "应落到第一个可见色块");
        cfg.set_group_visible(ToolbarGroup::Colors, false);
        assert_eq!(cfg.settle_color(1), 1, "一个色块都不可见时保留原颜色");
    }

    /// 只存隐藏集合：新元素（不在隐藏集合里）自动可见；未知键无害。
    #[test]
    fn unknown_keys_are_harmless() {
        let cfg: ToolbarConfig = toml::from_str(r#"hidden = ["tool.this_one_is_gone"]"#).unwrap();
        for g in ToolbarGroup::ALL {
            assert_eq!(cfg.group_state(g), GroupState::All, "未知键不应藏掉任何元素");
        }
    }

    /// 配置往返：`[toolbar] hidden = [...]` 读得回、存得住（旧配置缺字段 → 全可见）。
    #[test]
    fn config_roundtrip() {
        let old: Config = toml::from_str("[hotkey]\nscreenshot_key = 0x70\n").unwrap();
        assert!(old.toolbar.hidden.is_empty(), "旧配置应回退全可见");

        let mut cfg = Config::default();
        cfg.toolbar.set_visible(ToolbarSlot::Tool(Tool::Ellipse), false);
        cfg.toolbar.set_visible(ToolbarSlot::Copy, false);
        let text = toml::to_string_pretty(&cfg).unwrap();
        assert!(text.contains("[toolbar]"), "应写成独立的 [toolbar] 段：{text}");
        let back: Config = toml::from_str(&text).unwrap();
        assert!(!back.toolbar.visible(ToolbarSlot::Tool(Tool::Ellipse)));
        assert!(!back.toolbar.visible(ToolbarSlot::Copy));
        assert!(back.toolbar.visible(ToolbarSlot::Tool(Tool::Rect)));
    }
}
