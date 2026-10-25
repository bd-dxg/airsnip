// SPDX-License-Identifier: GPL-3.0-only
//! 设置面板控件模型与布局（M3）：左侧分类导航 + 右侧内容区。

use crate::overlay::geometry::SelRect;

mod keys;
pub use keys::*;

/// 窗口内容总宽（横向桌面窗口，宽高比约 1.4）。
pub const W: i32 = 720;
/// 窗口内容总高：固定值，避免切换分类时窗口上下跳动；
/// 内容更高的分类（快捷键 10 行）按实际内容兜底。
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
/// 支持项目行高度（文案 + 赞赏码，高于标准行）。
pub const QR_ROW_H: i32 = 200;

/// 分类页。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Section {
    General,
    Hotkeys,
    Save,
    Appearance,
    About,
}

impl Section {
    pub const ALL: [Section; 5] = [
        Section::General,
        Section::Hotkeys,
        Section::Save,
        Section::Appearance,
        Section::About,
    ];

    pub fn title(self) -> &'static str {
        match self {
            Section::General => "通用",
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
    /// 语言展示（一期仅 zh-CN）。
    Lang,
    /// 截图热键。
    HotkeyShot,
    /// 贴图热键。
    HotkeyPin,
    /// 贴图边框切换热键（M2b，默认 Ctrl+F3）。
    HotkeyBorder,
    /// 工具切换键（M3）：矩形 / 箭头 / 文本 / 模糊 / 荧光笔 / 高亮。
    ToolRect,
    ToolArrow,
    ToolText,
    ToolBlur,
    ToolHighlight,
    ToolGlow,
    /// 无修饰键风险提示（只读）。
    Widenote,
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
    /// 支持项目（关于页：文案 + 微信赞赏码）。
    Reward,
}

/// 行控件类型。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum RowKind {
    /// 开关（整行可点）。
    Toggle,
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
    /// 整行矩形（命中 + 悬停高亮）。
    pub rect: SelRect,
    /// 右侧控件区（开关/按键框/说明文本）。
    pub control: SelRect,
}

/// 面板内容布局：导航项 + 当前分类的行 + 总高度。
#[derive(Debug)]
pub struct Layout {
    pub items: Vec<Item>,
    pub height: i32,
}

/// 内容行几何（`s` 为 DPI 缩放系数，x 从导航栏右缘开始）。
fn mk_row(id: RowId, kind: RowKind, label: &'static str, y: i32, s: f32) -> Row {
    // 横向几何同样按 DPI 缩放，否则高 DPI 下内容区宽度不变、行高变大（比例失真）
    let h = (ROW_H as f32 * s) as i32;
    let w = (W as f32 * s) as i32;
    let nav_w = (NAV_W as f32 * s) as i32;
    let pad = (PAD as f32 * s) as i32;
    let cw = (260.0 * s) as i32;
    Row {
        id,
        kind,
        label,
        rect: SelRect {
            x: nav_w,
            y,
            w: w - nav_w - pad,
            h,
        },
        control: SelRect {
            x: w - pad - cw,
            y,
            w: cw,
            h,
        },
    }
}

/// 高行：高度按内容给（支持项目块），几何与标准行一致。
fn mk_row_tall(id: RowId, kind: RowKind, label: &'static str, y: i32, s: f32, h: i32) -> Row {
    let mut row = mk_row(id, kind, label, y, s);
    row.rect.h = h;
    row.control.h = h;
    row
}

/// 指定分类的内容行。
fn section_rows(section: Section, y0: i32, s: f32, items: &mut Vec<Item>) {
    let mut y = y0;
    let push = |id: RowId, kind: RowKind, label: &'static str, y: &mut i32, items: &mut Vec<Item>| {
        items.push(Item::Row(mk_row(id, kind, label, *y, s)));
        *y += (ROW_H as f32 * s) as i32;
    };
    match section {
        Section::General => {
            push(RowId::Autostart, RowKind::Toggle, "开机自启", &mut y, items);
            push(RowId::Lang, RowKind::Readonly, "语言", &mut y, items);
            push(RowId::PinPosOriginal, RowKind::Radio, "贴图位置·原位置", &mut y, items);
            push(RowId::PinPosCenter, RowKind::Radio, "贴图位置·屏幕中间", &mut y, items);
        }
        Section::Hotkeys => {
            push(RowId::HotkeyShot, RowKind::KeyCap, "截图", &mut y, items);
            push(RowId::HotkeyPin, RowKind::KeyCap, "贴图", &mut y, items);
            push(RowId::HotkeyBorder, RowKind::KeyCap, "贴图边框·窗口内", &mut y, items);
            push(RowId::ToolRect, RowKind::KeyCap, "工具·矩形", &mut y, items);
            push(RowId::ToolArrow, RowKind::KeyCap, "工具·箭头", &mut y, items);
            push(RowId::ToolText, RowKind::KeyCap, "工具·文本", &mut y, items);
            push(RowId::ToolBlur, RowKind::KeyCap, "工具·模糊", &mut y, items);
            push(RowId::ToolHighlight, RowKind::KeyCap, "工具·荧光笔", &mut y, items);
            push(RowId::ToolGlow, RowKind::KeyCap, "工具·聚光灯", &mut y, items);
            push(RowId::Widenote, RowKind::Readonly, "提示", &mut y, items);
        }
        Section::Save => {
            push(RowId::NameDateTime, RowKind::Radio, "文件名·日期 + 时间", &mut y, items);
            push(
                RowId::NameTimestamp,
                RowKind::Radio,
                "文件名·时间戳（毫秒）",
                &mut y,
                items,
            );
            push(RowId::NameDate, RowKind::Radio, "文件名·仅日期", &mut y, items);
        }
        Section::Appearance => {
            push(RowId::ThemeFollow, RowKind::Radio, "跟随系统", &mut y, items);
            push(RowId::ThemeLight, RowKind::Radio, "浅色", &mut y, items);
            push(RowId::ThemeDark, RowKind::Radio, "深色", &mut y, items);
        }
        Section::About => {
            push(RowId::About, RowKind::About, "jietu 截图工具", &mut y, items);
            push(RowId::Repo, RowKind::Link, "项目地址", &mut y, items);
            push(RowId::Licenses, RowKind::Link, "第三方许可", &mut y, items);
            let qh = (QR_ROW_H as f32 * s) as i32;
            items.push(Item::Row(mk_row_tall(RowId::Reward, RowKind::Qr, "支持项目", y, s, qh)));
        }
    }
}

/// 构建布局：导航项 + 当前节内容；高度固定为 `H`（内容更高时按内容兜底）。
pub fn layout(section: Section, scale: f32) -> Layout {
    let s = scale;
    let mut items = Vec::new();
    for sec in Section::ALL {
        items.push(Item::Nav(sec));
    }
    // 内容区：节标题 + 各节最大行数（每节各自累计，取最高者）
    let mut max_content = 0i32;
    for sec in Section::ALL {
        let mut rows = Vec::new();
        section_rows(sec, 0, s, &mut rows);
        let h = (CONTENT_HEADER_H as f32 * s) as i32 + rows.len() as i32 * (ROW_H as f32 * s) as i32;
        max_content = max_content.max(h);
    }
    // 当前节的内容行（实际绘制用）
    section_rows(section, (CONTENT_HEADER_H as f32 * s) as i32, s, &mut items);

    let nav_h = (NAV_H as f32 * s) as i32 * Section::ALL.len() as i32;
    let fixed_h = (H as f32 * s) as i32;
    Layout {
        items,
        height: fixed_h.max(max_content).max(nav_h),
    }
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
                    RowId::Lang,
                    RowId::PinPosOriginal,
                    RowId::PinPosCenter,
                ],
                Section::Hotkeys => vec![
                    RowId::HotkeyShot,
                    RowId::HotkeyPin,
                    RowId::HotkeyBorder,
                    RowId::ToolRect,
                    RowId::ToolArrow,
                    RowId::ToolText,
                    RowId::ToolBlur,
                    RowId::ToolHighlight,
                    RowId::ToolGlow,
                    RowId::Widenote,
                ],
                Section::Save => vec![RowId::NameDateTime, RowId::NameTimestamp, RowId::NameDate],
                Section::Appearance => vec![RowId::ThemeFollow, RowId::ThemeLight, RowId::ThemeDark],
                Section::About => vec![RowId::About, RowId::Repo, RowId::Licenses, RowId::Reward],
            };
            assert_eq!(rows, expected, "节 {sec:?} 行集合错误");
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
                        assert!(r.control.x >= r.rect.x, "控件区应在行内");
                    }
                }
            }
        }
    }
}
