// SPDX-License-Identifier: GPL-3.0-only
//! 覆盖层工具栏（EDT-7 / EDT-16）：类型、命中测试与绘制入口；布局见 `layout`，绘制见 `draw`。
//! 所有尺寸以 1080p / 100% DPI 为基准，经 `Ui` 按显示器 DPI 缩放（CAP-5）。

pub(crate) mod colorpanel;
mod draw;
pub(crate) mod fontlist;
mod label;
mod layout;
pub(crate) mod tip;

use std::sync::Arc;

use tiny_skia::Pixmap;

use crate::editor::{SWATCH_COUNT, Tool};
use crate::overlay::geometry::SelRect;
use crate::overlay::ui::Ui;
use crate::theme::Palette;

/// 第一行高度（工具 / 颜色 / 开关 / 输出按钮）。
const H_BASE: f32 = 34.0;
/// 文本第二行高度（EDT-4：字体族 / 粗体 / 斜体 / 字号）。
const H2_BASE: f32 = 30.0;
/// 两行之间的间距。
const ROW_GAP_BASE: f32 = 3.0;
/// 字体族按钮宽度（超出时截断加省略号）。
pub(super) const FONT_BTN_W_BASE: f32 = 132.0;
/// 字号显示格宽度。
const SIZE_W_BASE: f32 = 34.0;
/// 方形按钮边长（工具、撤销、开关）。
const BTN_BASE: f32 = 26.0;
/// 色块边长。
const SW_BASE: f32 = 22.0;
/// 元素间距。
const GAP_BASE: f32 = 4.0;
/// 组分隔区宽度。
const SEP_BASE: f32 = 9.0;
/// 内边距。
const PAD_BASE: f32 = 5.0;

/// 工具栏当前状态（决定高亮与可用性）。
#[derive(Clone, PartialEq)]
pub struct State {
    pub tool: Tool,
    pub color: usize,
    /// 色块颜色（EDT-7）：绘制色块用。
    pub colors: [[u8; 4]; SWATCH_COUNT],
    /// 矩形 / 椭圆是否填充（EDT-1/EDT-14）。
    pub filled: bool,
    /// 填充开关是否可用（EDT-1/EDT-14）：当前工具能画可填充形状，或选中对象是矩形 / 椭圆。
    /// 由此处统一裁决，绘制侧不再自己判一遍（见 `Overlay::fill_available`）。
    pub fill_enabled: bool,
    pub can_undo: bool,
    pub can_redo: bool,
    /// 第二行（EDT-4）：当前字体族、粗体/斜体、字号与字体面板是否展开。
    pub font_family: Arc<str>,
    pub bold: bool,
    pub italic: bool,
    pub font_size: f32,
    pub font_open: bool,
    /// 悬停中的按钮（EDT-10）：立即给底色，气泡延迟出现。
    pub hover: Option<Action>,
}

/// 工具栏动作。
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Action {
    Tool(Tool),
    Color(usize),
    ToggleFill,
    Undo,
    Redo,
    /// PIN-1：把选区（含标注）贴为置顶窗口。
    Pin,
    /// M4：把当前选区作为滚动视口，进入长截图拼接（长截图键同效，默认 S）。
    Longshot,
    /// OUT-1：复制选区到剪贴板（Enter / Ctrl+C 同效）。
    Copy,
    /// OUT-2：按配置的目录与格式保存（Ctrl+S 同效）。
    Save,
    /// EDT-4：展开/收起字体列表面板。
    FontPick,
    /// EDT-4：粗体开关。
    ToggleBold,
    /// EDT-4：斜体开关。
    ToggleItalic,
    /// EDT-4：字号减小 / 增大（每步 2px）。
    FontSmaller,
    FontLarger,
    /// EDT-4：字号数值格（仅展示，不响应点击）。
    FontSize,
}

/// 工具栏布局：整体矩形（含第二行）+ 各元素命中区。
#[derive(Clone, Debug)]
pub struct Toolbar {
    /// 整个工具栏（两行的并集，绘制背景与脏区共用）。
    pub rect: SelRect,
    /// 第一行矩形。
    pub first: SelRect,
    /// 第二行矩形（仅文本工具 / 选中文本对象时存在）。
    pub second: Option<SelRect>,
    /// 界面缩放：绘制时的圆角、内缩、分隔线与字号按它缩放（CAP-5）。
    pub ui: Ui,
    /// 元素间距（分隔线定位用）。
    pub(crate) gap: i32,
    /// 命中区（draw 子模块读取所有元素）。
    pub(crate) hits: Vec<(SelRect, Action)>,
    /// 组分隔线的 x（屏幕坐标）：每组首元素的位置，首个实际绘制的组不画（EDT-16）。
    pub(crate) dividers: Vec<i32>,
}

impl Toolbar {
    /// 命中测试：返回落在哪个元素上。
    pub fn hit(&self, x: i32, y: i32) -> Option<Action> {
        self.hits
            .iter()
            .find(|(r, _)| x >= r.x && x < r.x + r.w && y >= r.y && y < r.y + r.h)
            .map(|(_, a)| *a)
    }
}

pub(crate) use layout::{Shown, layout_ex};

/// 把工具栏绘制到覆盖层像素图（委托给 draw 子模块）。
pub fn draw(pixmap: &mut Pixmap, bar: &Toolbar, state: &State, palette: &Palette) {
    draw::render(pixmap, bar, state, palette);
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::overlay::geometry::{intersect_rect, text_rect};
    use crate::settings::{ToolbarConfig, ToolbarGroup, ToolbarSlot};

    /// 基准缩放（100% DPI）：尺寸与改动前的硬编码值一致。
    fn base() -> Ui {
        Ui::from_dpi(96)
    }

    fn sel(x: i32, y: i32, w: i32, h: i32) -> SelRect {
        SelRect { x, y, w, h }
    }

    /// 默认全可见（EDT-16）：`vis` 传 `ToolbarConfig::default()` 即得与加此功能前一致的布局。
    fn shown<'a>(vis: &'a ToolbarConfig, text_row: bool) -> Shown<'a> {
        Shown {
            vis,
            text_row,
            show_output: true,
        }
    }

    /// 普通截图场景的布局（工具栏必须存在）。
    fn layout(s: SelRect, vw: i32, vh: i32, vis: &ToolbarConfig, text_row: bool, ui: Ui) -> Toolbar {
        layout_ex(s, vw, vh, &shown(vis, text_row), ui).expect("元素未全隐藏时应有工具栏")
    }

    /// 全可见时的工具栏配置。
    fn all() -> ToolbarConfig {
        ToolbarConfig::default()
    }

    /// 窄屏 / 高缩放下的越界回归（EDT-7/EDT-16）：工具栏从 19 个按钮长到 23 个后更宽，
    /// 必须仍完整落在屏幕内（不得被右侧裁掉，也不得顶出上 / 下边）。
    #[test]
    fn bar_stays_on_screen_on_narrow_displays() {
        let mut minimal = ToolbarConfig::default();
        minimal.set_group_visible(ToolbarGroup::Tools, false);
        minimal.set_group_visible(ToolbarGroup::Colors, false);
        minimal.set_group_visible(ToolbarGroup::Edit, false);
        for (vw, vh, dpi) in [(1024, 768, 96), (1280, 800, 120), (1920, 1080, 192), (2560, 1440, 192)] {
            let ui = Ui::from_dpi(dpi);
            for vis in [all(), minimal.clone()] {
                for text_row in [false, true] {
                    // 小选区 + 贴右下角：工具栏既宽又无处可翻，最容易越界
                    for s in [sel(60, 60, 40, 40), sel(vw - 120, vh - 120, 60, 60)] {
                        let bar = layout(s, vw, vh, &vis, text_row, ui);
                        assert!(bar.rect.x >= 0, "{vw}x{vh}@{dpi} 左越界：{bar:?}");
                        assert!(
                            bar.rect.x + bar.rect.w <= vw,
                            "{vw}x{vh}@{dpi} 右越界（选区 {s:?}）：{bar:?}"
                        );
                        assert!(bar.rect.y >= 0, "{vw}x{vh}@{dpi} 上越界：{bar:?}");
                        assert!(
                            bar.rect.y + bar.rect.h <= vh,
                            "{vw}x{vh}@{dpi} 下越界（选区 {s:?}）：{bar:?}"
                        );
                    }
                }
            }
        }
    }

    /// 工具栏默认贴在选区下方（上下都有空间时不能跑到上方）。
    #[test]
    fn toolbar_defaults_below_selection() {
        let (vw, vh) = (2560, 1440);
        for text_row in [false, true] {
            for s in [sel(600, 500, 800, 400), sel(600, 60, 800, 400)] {
                let bar = layout(s, vw, vh, &all(), text_row, base());
                assert!(
                    bar.rect.y >= s.y + s.h,
                    "工具栏应在选区下方（text_row={text_row}, 选区 {s:?}）：{bar:?}"
                );
            }
        }
    }

    /// 工具栏不能与信息文字重叠，否则文字会盖在工具栏上（见 overlay::text_rect）。
    #[test]
    fn toolbar_never_overlaps_info_text() {
        let (vw, vh) = (2560, 1440);
        let cases = [
            sel(100, 100, 400, 300),   // 常规：文字在上、工具栏在下
            sel(100, 4, 400, 300),     // 贴顶：文字在下，工具栏再往下让一个文字高度
            sel(100, 100, 900, 1300),  // 贴底：工具栏翻到文字上方
            sel(100, 4, 900, 1420),    // 贴顶贴底：工具栏翻到上方、文字在下方
            sel(2400, 100, 150, 1200), // 靠右：工具栏贴右边
        ];
        for ui in [base(), Ui::from_dpi(192)] {
            for s in cases {
                let bar = layout(s, vw, vh, &all(), false, ui).rect;
                let text = text_rect(s, vw, vh, bar.h, ui);
                assert!(
                    intersect_rect(bar, text).is_none(),
                    "工具栏 {bar:?} 与信息文字 {text:?} 重叠（选区 {s:?}）"
                );
                assert!(bar.y >= 0 && bar.y + bar.h <= vh, "工具栏应在屏幕内：{bar:?}");
                assert!(bar.x >= 0 && bar.x + bar.w <= vw, "工具栏应在屏幕内：{bar:?}");
            }
        }
    }

    /// 第二行（EDT-4）同样不能与信息文字重叠，且两行不越出屏幕。
    #[test]
    fn text_row_stays_inside_screen() {
        // 1080p@200%：工具栏上元素全可见时已经很宽，
        // 1366×768@200% 已放不下，那个组合由 `bar_wider_than_screen_starts_at_margin` 单独记录
        for (vw, vh, ui) in [(1366, 768, base()), (1920, 1080, Ui::from_dpi(192))] {
            let cases = [
                sel(100, 100, 400, 300),
                sel(100, 4, 400, 300),
                sel(100, 100, 900, 600),
                sel(100, 4, 900, 700),
                sel(1000, 300, 300, 400),
            ];
            for s in cases {
                let bar = layout(s, vw, vh, &all(), true, ui);
                let text = text_rect(s, vw, vh, bar.rect.h, ui);
                assert!(
                    intersect_rect(bar.rect, text).is_none(),
                    "工具栏 {bar:?} 与信息文字 {text:?} 重叠"
                );
                let second = bar.second.expect("文本工具应有第二行");
                assert_eq!(second.y, bar.first.y + bar.first.h + ui.px(ROW_GAP_BASE));
                assert!(
                    bar.rect.y >= 0 && bar.rect.y + bar.rect.h <= vh,
                    "工具栏应在屏幕内：{bar:?}"
                );
                assert!(
                    bar.rect.x >= 0 && bar.rect.x + bar.rect.w <= vw,
                    "工具栏应在屏幕内：{bar:?}"
                );
                // 第二行元素全在第二行矩形内
                for (r, _) in bar.hits.iter().filter(|(r, _)| r.y >= second.y) {
                    assert!(
                        r.x >= second.x && r.x + r.w <= second.x + second.w,
                        "第二行元素 {r:?} 超出第二行 {second:?}"
                    );
                }
            }
        }
    }

    /// 已知限制（EDT-16）：工具栏比屏幕还宽时只能从左边距开始画，右侧被裁（**不**引入换行、
    /// 也**不**改为右对齐）。这是「工具栏元素全可见」的副作用，已写入 README「已知限制」
    /// 与 ADR 0033：用户的出路是隐藏不用的元素（见下一条测试）。
    /// 用 1280×720@200%：移除 OCR 按钮后 1366×768@200% 已经放得下（宽 1332），
    /// 但更小的屏仍然会超。
    #[test]
    fn bar_wider_than_screen_is_left_aligned() {
        let (vw, vh) = (1280, 720);
        let ui = Ui::from_dpi(192);
        let bar = layout(sel(100, 100, 400, 300), vw, vh, &all(), true, ui);
        assert!(bar.rect.w > vw, "该组合下工具栏确实比屏幕宽：{}", bar.rect.w);
        assert_eq!(bar.rect.x, ui.px(4.0), "不得从负坐标开始画（右侧被裁而非左侧）");
        assert!(
            bar.rect.y >= 0 && bar.rect.y + bar.rect.h <= vh,
            "纵向仍应完整落在屏幕内"
        );
    }

    /// 同上的出路（EDT-16 R1）：隐藏整组后工具栏重新放得下，且仍不越出左边距。
    #[test]
    fn hiding_groups_fits_narrow_hidpi_screen() {
        let (vw, vh) = (1366, 768);
        let ui = Ui::from_dpi(192);
        let s = sel(100, 100, 400, 300);
        let mut vis = all();
        vis.set_group_visible(ToolbarGroup::Colors, false);
        vis.set_group_visible(ToolbarGroup::Output, false);
        let thin = layout(s, vw, vh, &vis, true, ui);
        assert!(thin.rect.w <= vw, "隐藏两组后应能放下：{}", thin.rect.w);
        assert!(thin.rect.x >= ui.px(4.0) && thin.rect.x + thin.rect.w <= vw);
        // 剩下的元素一个不少地仍在命中区里
        assert!(thin.hits.iter().any(|(_, a)| *a == Action::Save));
        assert!(!thin.hits.iter().any(|(_, a)| *a == Action::Pin));
    }

    #[test]
    fn toolbar_hit_maps_each_element() {
        let bar = layout(sel(100, 100, 400, 300), 2560, 1440, &all(), true, base());
        // 每个元素的中心点必须命中它自己
        for (r, action) in &bar.hits {
            let (cx, cy) = (r.x + r.w / 2, r.y + r.h / 2);
            assert_eq!(bar.hit(cx, cy), Some(*action), "元素 {action:?} 中心未命中自己");
        }
        // 元素不重叠（命中区连续递增；第二行另起一排列，只比较同行）
        let second_y = bar.second.expect("应有第二行").y;
        for w in bar.hits.windows(2) {
            if w[0].0.y >= second_y && w[1].0.y >= second_y {
                assert!(w[0].0.x + w[0].0.w <= w[1].0.x, "元素重叠：{:?} / {:?}", w[0].0, w[1].0);
            }
        }
        // 元素顺序与数量（EDT-7 工具栏结构 + EDT-4 第二行）
        let actions: Vec<Action> = bar.hits.iter().map(|(_, a)| *a).collect();
        assert_eq!(actions.len(), 28);
        assert_eq!(actions[..10], Tool::ALL.map(Action::Tool));
        // 色块：5 个，均可双击改色（issue #73 后续，不再有单独的自定义槽）
        assert_eq!(actions[10..15], [0, 1, 2, 3, 4].map(Action::Color));
        assert_eq!(actions[15], Action::ToggleFill);
        assert_eq!(actions[16], Action::Undo);
        assert_eq!(actions[17], Action::Redo);
        assert_eq!(actions[18], Action::Pin);
        assert_eq!(actions[19], Action::Longshot);
        assert_eq!(actions[20], Action::Copy);
        assert_eq!(actions[21], Action::Save);
        assert_eq!(
            actions[22..],
            [
                Action::FontPick,
                Action::ToggleBold,
                Action::ToggleItalic,
                Action::FontSmaller,
                Action::FontSize,
                Action::FontLarger
            ]
        );
    }

    /// 非文本场景第二行不应出现（避免占地与多余命中区）。
    #[test]
    fn second_row_only_when_text_row_requested() {
        let bar = layout(sel(100, 100, 400, 300), 2560, 1440, &all(), false, base());
        assert!(bar.second.is_none());
        assert_eq!(bar.rect.h, base().px(H_BASE));
        assert_eq!(bar.hits.len(), 22);
    }

    /// 隐藏的元素不再有命中区，工具栏宽度随之变窄（R1/R2）。
    #[test]
    fn hidden_elements_drop_hits_and_shrink_width() {
        let s = sel(100, 100, 400, 300);
        let full = layout(s, 2560, 1440, &all(), false, base());
        let mut vis = all();
        vis.set_visible(ToolbarSlot::Tool(Tool::Ellipse), false);
        vis.set_visible(ToolbarSlot::Color(0), false);
        vis.set_visible(ToolbarSlot::Save, false);
        let thin = layout(s, 2560, 1440, &vis, false, base());
        for hidden in [Action::Tool(Tool::Ellipse), Action::Color(0), Action::Save] {
            assert!(
                !thin.hits.iter().any(|(_, a)| *a == hidden),
                "隐藏后不应再有 {hidden:?} 的命中区"
            );
            assert!(full.hits.iter().any(|(_, a)| *a == hidden), "全可见时应有 {hidden:?}");
        }
        let btn = base().px(BTN_BASE);
        let sw = base().px(SW_BASE);
        let gap = base().px(GAP_BASE);
        // 宽度 = Σ宽 + (n-1)·间距 + 分隔 + 2·内边距：去掉两个按钮 + 一个色块，连带它们的间距
        assert_eq!(
            thin.rect.w,
            full.rect.w - (btn * 2 + sw + gap * 3),
            "宽度应按可见项重算"
        );
        assert_eq!(thin.hits.len(), full.hits.len() - 3);
    }

    /// 全部元素隐藏 → 没有工具栏（R4）；只剩一个元素时仍照常布局。
    #[test]
    fn all_hidden_means_no_toolbar() {
        let s = sel(100, 100, 400, 300);
        let mut vis = all();
        for g in ToolbarGroup::ALL {
            vis.set_group_visible(g, false);
        }
        assert!(layout_ex(s, 2560, 1440, &shown(&vis, true), base()).is_none());

        vis.set_visible(ToolbarSlot::Save, true);
        let bar = layout(s, 2560, 1440, &vis, true, base());
        assert_eq!(bar.hits.len(), 1);
        assert_eq!(bar.rect.h, base().px(H_BASE));
        assert_eq!(
            bar.rect.w,
            base().px(BTN_BASE) + base().px(PAD_BASE) * 2,
            "只剩一个按钮的宽度"
        );
    }

    /// 第二行整行一个开关（R1）：隐藏后即使文本工具激活也不出现，且不占高度。
    #[test]
    fn text_row_switch_hides_whole_row() {
        let s = sel(100, 100, 400, 300);
        let mut vis = all();
        vis.set_visible(ToolbarSlot::TextRow, false);
        let bar = layout(s, 2560, 1440, &vis, true, base());
        assert!(bar.second.is_none(), "第二行开关隐藏后不应出现第二行");
        assert_eq!(bar.rect.h, base().px(H_BASE), "不应为第二行留高度");
        assert!(!bar.hits.iter().any(|(_, a)| *a == Action::FontLarger));
    }

    /// 组分隔线只画在实际绘制的组之间（整组隐藏时不留下孤立的竖线）。
    #[test]
    fn dividers_follow_drawn_groups() {
        let s = sel(100, 100, 400, 300);
        let full = layout(s, 2560, 1440, &all(), false, base());
        assert_eq!(full.dividers.len(), ToolbarGroup::ALL.len() - 1, "5 组应有 4 条分隔线");
        assert!(full.dividers.windows(2).all(|w| w[0] < w[1]), "分隔线应自左向右递增");
        assert!(full.dividers.iter().all(|x| *x > full.rect.x));

        let mut vis = all();
        vis.set_group_visible(ToolbarGroup::Colors, false);
        let no_colors = layout(s, 2560, 1440, &vis, false, base());
        assert_eq!(
            no_colors.dividers.len(),
            ToolbarGroup::ALL.len() - 2,
            "少一组就少一条线"
        );
    }

    /// 贴图编辑模式（ADR 0028）：输出组（贴图 / 长截图）整体不出现，复制/保存仍在。
    #[test]
    fn output_group_hidden_when_editing() {
        let s = sel(100, 100, 400, 300);
        let vis = all();
        let full = layout_ex(s, 2560, 1440, &shown(&vis, false), base()).expect("应有工具栏");
        let editing = Shown {
            vis: &vis,
            text_row: false,
            show_output: false,
        };
        let edit = layout_ex(s, 2560, 1440, &editing, base()).expect("应有工具栏");
        let is_output = |a: &Action| matches!(a, Action::Pin | Action::Longshot);
        assert!(full.hits.iter().any(|(_, a)| *a == Action::Pin));
        assert!(!edit.hits.iter().any(|(_, a)| is_output(a)), "编辑模式不应有输出组");
        assert!(edit.hits.iter().any(|(_, a)| *a == Action::Copy));
        assert!(edit.hits.iter().any(|(_, a)| *a == Action::Save));
        // 少了两个按钮，工具栏更窄
        assert!(edit.rect.w < full.rect.w);
    }

    /// 高 DPI（200%）下工具栏整体尺寸翻倍：4K 屏上控件不再只有一半大（CAP-5）。
    #[test]
    fn toolbar_scales_with_dpi() {
        let s = sel(600, 500, 800, 400);
        let base_bar = layout(s, 2560, 1440, &all(), true, base());
        let hidpi = layout(s, 3840, 2160, &all(), true, Ui::from_dpi(192));
        assert_eq!(hidpi.first.h, base_bar.first.h * 2, "第一行高度应翻倍");
        assert_eq!(hidpi.rect.h, base_bar.rect.h * 2, "整体高度应翻倍");
        assert_eq!(hidpi.rect.w, base_bar.rect.w * 2, "整体宽度应翻倍");
        // 每个命中区同样翻倍（按钮仍可点，且命中范围与绘制一致）
        for (a, b) in base_bar.hits.iter().zip(hidpi.hits.iter()) {
            assert_eq!(a.1, b.1, "元素顺序应一致");
            assert_eq!(a.0.w * 2, b.0.w, "元素宽度应翻倍：{:?}", a.1);
            assert_eq!(a.0.h * 2, b.0.h, "元素高度应翻倍：{:?}", a.1);
        }
    }
}
