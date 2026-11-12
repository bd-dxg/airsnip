// SPDX-License-Identifier: GPL-3.0-only
//! 设置面板控件模型与布局（M3）：左侧分类导航 + 右侧内容区。

use crate::overlay::geometry::SelRect;
use crate::settings::{ToolbarGroup, ToolbarSlot};

mod keyrows;
mod keys;
mod layout;
mod scale;
pub use keyrows::*;
pub use keys::*;
pub use layout::*;
pub use scale::*;

/// 窗口内容总宽（横向桌面窗口，宽高比约 1.4）。
pub const W: i32 = 720;
/// 窗口内容总高：固定值，避免切换分类时窗口上下跳动。
/// 最高的分类「快捷键」采用两列排布（见 `layout`），行数受控，必须落在此高内，
/// 否则窗口会被内容撑高、变成近正方形（桌面窗口应保持宽 > 高）。
pub const H: i32 = 520;
/// 左侧导航栏宽。
pub const NAV_W: i32 = 124;
/// 左右边距。
pub const PAD: i32 = 24;
/// 导航项高度。
pub const NAV_H: i32 = 44;
/// 右侧内容区顶部节标题高度。
pub const CONTENT_HEADER_H: i32 = 40;
/// 内容行高度。
pub const ROW_H: i32 = 46;
/// 两列排布时的列间距（快捷键页，见 `layout`）。
pub const GRID_GAP: i32 = 8;
/// 单元格内容的水平内边距（两列排布：单元格比整行窄，内边距略小）。
pub const CELLPAD: i32 = 16;
/// 悬停高亮矩形相对内容外扩的距离（让高亮四周留出余量，不再紧贴控件）。
pub const HOVER_PAD: i32 = 8;
/// 支持项目行高度（文案 + 赞赏码，高于标准行）。
pub const QR_ROW_H: i32 = 200;
/// 「工具栏」页的组头行高度（比标准行矮，五组标题不占太多竖向空间）。
pub const GROUP_H: i32 = 26;
/// 「工具栏」页单元格的水平内边距（多列布局，比整行小）。
pub const CELL_PAD: i32 = 6;
/// 开关胶囊宽度（绘制与命中共用，避免两处各写一份）。
pub const TOGGLE_W: i32 = 36;
/// 快捷键行右侧清除按钮边长（与键帽同高，视觉上齐平）。
pub const CLEAR_BTN: i32 = 30;
/// 键帽与清除按钮之间的间隔。
pub const CLEAR_GAP: i32 = 8;

/// 分类页。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Section {
    General,
    /// 工具栏可见性（EDT-16，issue #143）：逐元素显隐 + 组头全显 / 全隐。
    Toolbar,
    Hotkeys,
    Save,
    Appearance,
    About,
}

impl Section {
    pub const ALL: [Section; 6] = [
        Section::General,
        Section::Toolbar,
        Section::Hotkeys,
        Section::Save,
        Section::Appearance,
        Section::About,
    ];

    pub fn title(self) -> &'static str {
        match self {
            Section::General => "通用",
            Section::Toolbar => "工具栏",
            Section::Hotkeys => "快捷键",
            Section::Save => "保存",
            Section::Appearance => "外观",
            Section::About => "关于",
        }
    }
}

/// 控件 ID。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum RowId {
    /// 开机自启开关。
    Autostart,
    /// 连按两次退出开关（OUT-3，issue #120）：有未保存标注时，同一键连按两次才退出。
    ConfirmCancel,
    /// 语言展示（一期仅 zh-CN）。
    Lang,
    /// 截图热键。
    HotkeyShot,
    /// 贴图热键。
    HotkeyPin,
    /// 再次截图键（覆盖层内按下，默认 Shift+F1，issue #118）：当前画面作为新底图重新框选。
    HotkeyRecapture,
    /// 贴图边框切换热键（M2b，默认 Ctrl+F3）。
    HotkeyBorder,
    /// 长截图键（覆盖层内按下，默认 S）。
    HotkeyLongshot,
    /// 工具切换键（M3）：矩形 / 箭头 / 文本 / 模糊 / 荧光笔 / 高亮。
    ToolRect,
    ToolArrow,
    ToolText,
    ToolBlur,
    ToolHighlight,
    ToolGlow,
    /// 工具切换键：序号标记（EDT-12）。
    ToolNumber,
    /// 工具切换键：直线（EDT-13）。
    ToolLine,
    /// 工具切换键：椭圆（EDT-14）。
    ToolEllipse,
    /// 工具切换键：手绘自由笔画（EDT-15）。
    ToolFreehand,
    /// 无修饰键风险提示（只读）。
    Widenote,
    /// 一键恢复默认快捷键（SYS-2，快捷键页底部）。
    ResetKeys,
    /// 贴图位置：截图原位置（M2b）。
    PinPosOriginal,
    /// 贴图位置：屏幕中间（M2b）。
    PinPosCenter,
    /// 主题：跟随系统。
    ThemeFollow,
    /// 主题：浅色。
    ThemeLight,
    /// 主题：深色。
    ThemeDark,
    /// 界面缩放（CAP-5）：自动（按显示器 DPI + 分辨率）。
    UiScaleAuto,
    /// 界面缩放：固定 100%。
    UiScale100,
    /// 界面缩放：固定 125%。
    UiScale125,
    /// 界面缩放：固定 150%。
    UiScale150,
    /// 界面缩放：固定 175%。
    UiScale175,
    /// 界面缩放：固定 200%。
    UiScale200,
    /// 文件名模板：日期 + 时间（OUT-2）。
    NameDateTime,
    /// 文件名模板：Unix 毫秒时间戳（OUT-2）。
    NameTimestamp,
    /// 文件名模板：仅日期（OUT-2）。
    NameDate,
    /// 关于（版本与许可）。
    About,
    /// 项目地址（关于页，点击用浏览器打开）。
    Repo,
    /// 第三方组件声明入口（M5，PRD §9）。
    Licenses,
    /// 检查更新（SYS-7）：右侧显示当前更新状态，整行可点。
    CheckUpdate,
    /// 启动时自动检查更新开关（SYS-7）。
    AutoCheckUpdate,
    /// 支持项目（关于页：文案 + 微信赞赏码）。
    Reward,
    /// 工具栏元素显隐开关（EDT-16）：一个元素一行（多列排布）。
    Toolbar(ToolbarSlot),
    /// 工具栏组头（EDT-16）：整组全显 / 全隐，部分可见时半选。
    ToolbarGroup(ToolbarGroup),
}

/// 行控件类型。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum RowKind {
    /// 开关（整行可点）。
    Toggle,
    /// 组头：组名 + 紧跟组名的三态勾选框（整组全显 / 全隐，EDT-16）。
    Group,
    /// 热键捕获按钮（点右侧区域进入捕获）。
    KeyCap,
    /// 单选（整行可点）。
    Radio,
    /// 只读：左侧标签 + 右侧说明文本。
    Readonly,
    /// 关于块：左侧产品名、右侧版本与许可。
    About,
    /// 外部链接：右侧强调色文字，点击用浏览器打开。
    Link,
    /// 状态行：右侧按钮（文案随状态变，忙时置灰），整行可点。
    Button,
    /// 支持项目块：高行（左侧文案 + 右侧赞赏码）。
    Qr,
}

/// 布局项：导航项或内容行。
#[derive(Debug)]
pub enum Item {
    Nav(Section),
    Row(Row),
}

/// 单行控件几何与标识。
#[derive(Debug)]
pub struct Row {
    pub id: RowId,
    pub kind: RowKind,
    pub label: &'static str,
    /// 标签左内边距（含 DPI 缩放）：整行为 `PAD`，两列单元格较小。
    pub label_pad: i32,
    /// 悬停高亮矩形：内容区四周外扩 `HOVER_PAD`（含圆角绘制），保证高亮有内边距。
    pub hover: SelRect,
    /// 整行矩形（命中区）。
    pub rect: SelRect,
    /// 右侧控件区（开关/按键框/说明文本）。
    pub control: SelRect,
    /// 行内清除按钮（SYS-2，仅 `KeyCap` 行有意义；其余行为空矩形）。
    /// 与 `control` 分开命中：点键帽进捕获态，点这里是直接清空。
    pub clear: SelRect,
}

/// 面板内容布局：导航项 + 当前分类的行 + 总高度。
#[derive(Debug)]
pub struct Layout {
    pub items: Vec<Item>,
    pub height: i32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layout_has_nav_and_rows() {
        let l = layout(Section::General, 1.0);
        let nav: Vec<_> = l
            .items
            .iter()
            .filter_map(|i| match i {
                Item::Nav(s) => Some(*s),
                _ => None,
            })
            .collect();
        assert_eq!(nav, Section::ALL);
        let rows: Vec<_> = l
            .items
            .iter()
            .filter_map(|i| match i {
                Item::Row(r) => Some(r.id),
                _ => None,
            })
            .collect();
        assert!(rows.contains(&RowId::Autostart));
        assert!(rows.contains(&RowId::Lang));
        assert!(l.height > 0);
    }

    /// 桌面窗口比例：任何分类页的内容都不得把窗口撑高（宽必须大于高），
    /// 行数最多的快捷键页（两列排布后 6 行 + 2 行）也必须落在固定高内。
    #[test]
    fn panel_stays_landscape_for_every_section() {
        for s in [1.0f32, 1.5, 2.0] {
            for sec in Section::ALL {
                let l = layout(sec, s);
                assert!(
                    l.height <= (H as f32 * s) as i32,
                    "{sec:?} @{s}x 内容 {} 超过固定高",
                    l.height
                );
                assert!(
                    l.height < (W as f32 * s) as i32,
                    "{sec:?} @{s}x 变成高 ≥ 宽：{} vs {}",
                    l.height,
                    (W as f32 * s) as i32
                );
            }
            assert_eq!(
                layout(Section::Hotkeys, s).height,
                (H as f32 * s) as i32,
                "@{s}x 快捷键页应落在固定高内"
            );
        }
    }

    /// 快捷键页两列排布：同一视觉行最多两项，总行数受控（不再每项独占一行）。
    #[test]
    fn hotkeys_use_two_columns() {
        for s in [1.0f32, 1.5, 2.0] {
            let l = layout(Section::Hotkeys, s);
            let mut ys: std::collections::BTreeMap<i32, usize> = std::collections::BTreeMap::new();
            let mut n = 0;
            for item in &l.items {
                let Item::Row(r) = item else { continue };
                if r.kind == RowKind::KeyCap {
                    n += 1;
                    *ys.entry(r.rect.y).or_insert(0) += 1;
                }
            }
            assert_eq!(n, KEY_ROWS.len(), "@{s}x 快捷键键帽行应仍为 {} 项", KEY_ROWS.len());
            assert!(ys.values().all(|&c| c <= 2), "@{s}x 同一行不应超过两项");
            assert!(ys.len() <= 9, "@{s}x 两列排布后行数应受控：{}", ys.len());
        }
    }

    #[test]
    fn each_section_has_its_rows() {
        for sec in Section::ALL {
            let l = layout(sec, 1.0);
            let rows: Vec<_> = l
                .items
                .iter()
                .filter_map(|i| match i {
                    Item::Row(r) => Some(r.id),
                    _ => None,
                })
                .collect();
            let expected = match sec {
                Section::General => vec![
                    RowId::Autostart,
                    RowId::ConfirmCancel,
                    RowId::Lang,
                    RowId::PinPosOriginal,
                    RowId::PinPosCenter,
                ],
                Section::Hotkeys => vec![
                    RowId::HotkeyShot,
                    RowId::HotkeyRecapture,
                    RowId::HotkeyPin,
                    RowId::HotkeyBorder,
                    RowId::HotkeyLongshot,
                    RowId::ToolRect,
                    RowId::ToolArrow,
                    RowId::ToolText,
                    RowId::ToolBlur,
                    RowId::ToolHighlight,
                    RowId::ToolGlow,
                    RowId::ToolNumber,
                    RowId::ToolLine,
                    RowId::ToolEllipse,
                    RowId::ToolFreehand,
                    RowId::ResetKeys,
                    RowId::Widenote,
                ],
                Section::Save => vec![RowId::NameDateTime, RowId::NameTimestamp, RowId::NameDate],
                Section::Appearance => vec![
                    RowId::ThemeFollow,
                    RowId::ThemeLight,
                    RowId::ThemeDark,
                    RowId::UiScaleAuto,
                    RowId::UiScale100,
                    RowId::UiScale125,
                    RowId::UiScale150,
                    RowId::UiScale175,
                    RowId::UiScale200,
                ],
                Section::About => vec![
                    RowId::About,
                    RowId::Repo,
                    RowId::Licenses,
                    RowId::CheckUpdate,
                    RowId::AutoCheckUpdate,
                    RowId::Reward,
                ],
                Section::Toolbar => {
                    // 组头 + 组内每个元素各一行（顺序即界面顺序，EDT-16）
                    let mut rows = Vec::new();
                    for g in ToolbarGroup::ALL {
                        rows.push(RowId::ToolbarGroup(g));
                        rows.extend(g.slots().iter().map(|s| RowId::Toolbar(*s)));
                    }
                    rows
                }
            };
            assert_eq!(rows, expected, "节 {sec:?} 行集合错误");
        }
    }

    /// 快捷键行的清除按钮（SYS-2）：只有键帽行有，且不压键帽、不越出控件区。
    #[test]
    fn keycap_rows_have_clear_button() {
        let l = layout(Section::Hotkeys, 1.0);
        let mut caps = 0;
        for item in &l.items {
            let Item::Row(r) = item else { continue };
            if r.kind == RowKind::KeyCap {
                caps += 1;
                assert!(r.clear.w > 0 && r.clear.h > 0, "{:?} 应有清除按钮", r.id);
                assert!(r.clear.x > r.control.x, "{:?} 清除按钮应在键帽右侧", r.id);
                assert!(
                    r.clear.x + r.clear.w <= r.control.x + r.control.w,
                    "{:?} 清除按钮不应越出控件区",
                    r.id
                );
            } else {
                assert_eq!(r.clear.w, 0, "{:?} 非键帽行不应有清除按钮", r.id);
            }
        }
        assert_eq!(caps, KEY_ROWS.len(), "快捷键页应有 {} 个键帽行", KEY_ROWS.len());
    }

    /// 任意缩放下键帽仍有足够宽度显示组合键文本。
    #[test]
    fn keycap_keeps_usable_width_at_any_scale() {
        for scale in [1.0f32, 1.25, 1.5, 2.0] {
            let l = layout(Section::Hotkeys, scale);
            for item in &l.items {
                if let Item::Row(r) = item
                    && r.kind == RowKind::KeyCap
                {
                    let gap = (CLEAR_GAP as f32 * scale) as i32;
                    let kb_w = r.control.w - r.clear.w - gap;
                    assert!(kb_w > 100, "{:?} 在 {scale}x 下键帽过窄：{kb_w}", r.id);
                    assert!(r.clear.w >= 20, "{:?} 在 {scale}x 下清除按钮过小", r.id);
                }
            }
        }
    }

    #[test]
    fn rows_do_not_overlap_nav() {
        let l = layout(Section::Hotkeys, 1.0);
        for item in &l.items {
            if let Item::Row(r) = item {
                assert!(r.rect.x >= NAV_W, "内容行不得进入导航区：{r:?}");
            }
        }
    }

    #[test]
    fn about_reward_row_is_taller_than_standard() {
        let l = layout(Section::About, 1.0);
        let reward = l
            .items
            .iter()
            .find_map(|i| match i {
                Item::Row(r) if r.id == RowId::Reward => Some(r),
                _ => None,
            })
            .expect("关于页应有支持项目行");
        assert_eq!(reward.rect.h, QR_ROW_H, "赞赏码行高度应为 QR_ROW_H");
        assert_eq!(reward.control.h, QR_ROW_H, "控件区高度应与行一致");
    }

    /// 「工具栏」页单元格必须容得下「标签 + 开关」（5 列很窄，标签太长会压到开关上）。
    /// 用真实字体量宽，改标签或改列数时能当场报错；每档缩放都查（单元格宽是整除，
    /// 余数随缩放变化，1.0 下放得下不等于 1.5 下也放得下）。
    #[test]
    fn toolbar_cells_fit_label_and_toggle() {
        let total = crate::settings::ToolbarSlot::all().count();
        for scale in [1.0f32, 1.25, 1.5, 2.0] {
            let l = layout(Section::Toolbar, scale);
            let mut cells = 0;
            for item in &l.items {
                let Item::Row(r) = item else { continue };
                let RowId::Toolbar(slot) = r.id else { continue };
                cells += 1;
                let spec = crate::render::text::FontSpec::new(16.0 * scale);
                let (label_w, _) = crate::render::text::measure(slot.label(), &spec);
                let avail = (r.control.x - (r.rect.x + r.label_pad)) as f32;
                assert!(
                    label_w <= avail,
                    "@{scale}x 「{}」宽 {label_w}px，超过单元格可用宽度 {avail}px",
                    slot.label()
                );
                assert!(
                    r.control.x + r.control.w <= r.rect.x + r.rect.w,
                    "@{scale}x 「{}」的开关超出单元格",
                    slot.label()
                );
            }
            assert_eq!(cells, total, "@{scale}x 每个元素都应有开关单元格");
        }
    }

    #[test]
    fn window_is_wider_than_tall() {
        for sec in Section::ALL {
            let l = layout(sec, 1.0);
            assert!(W > l.height, "{sec:?} 窗口应横向（宽 {W} vs 高 {}）", l.height);
        }
    }

    #[test]
    fn rows_fit_inside_window_at_any_scale() {
        for scale in [1.0f32, 1.25, 1.5, 2.0] {
            let w = (W as f32 * scale) as i32;
            for sec in Section::ALL {
                let l = layout(sec, scale);
                for item in &l.items {
                    if let Item::Row(r) = item {
                        assert!(r.rect.x + r.rect.w <= w, "{sec:?} 行超出右边界");
                        assert!(r.rect.y + r.rect.h <= l.height, "{sec:?} 行超出下边界");
                        assert!(r.control.x >= r.rect.x, "{:?} @{scale}x 控件区应在行内", r.id);
                    }
                }
            }
        }
    }
}
