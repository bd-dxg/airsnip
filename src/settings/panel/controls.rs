// SPDX-License-Identifier: GPL-3.0-only
//! 设置面板控件模型与布局（M3）：左侧分类导航 + 右侧内容区。

use crate::overlay::geometry::SelRect;

mod keys;
mod layout;
mod scale;
pub use keys::*;
pub use layout::*;
pub use scale::*;

/// 窗口内容总宽（横向桌面窗口，宽高比约 1.4）。
pub const W: i32 = 720;
/// 窗口内容总高：固定值，避免切换分类时窗口上下跳动；
/// 内容更高的分类（快捷键 11 行）按实际内容兜底。
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
/// 快捷键行右侧清除按钮边长（与键帽同高，视觉上齐平）。
pub const CLEAR_BTN: i32 = 30;
/// 键帽与清除按钮之间的间隔。
pub const CLEAR_GAP: i32 = 8;

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
    /// 长截图键（覆盖层内按下，默认 S）。
    HotkeyLongshot,
    /// 工具切换键（M3）：矩形 / 箭头 / 文本 / 模糊 / 荧光笔 / 高亮。
    ToolRect,
    ToolArrow,
    ToolText,
    ToolBlur,
    ToolHighlight,
    ToolGlow,
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
    /// 操作按钮：右侧强调色文字，点击执行一次动作（如「恢复默认」）。
    Action,
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
    /// 整行矩形（命中 + 悬停高亮）。
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
                    RowId::HotkeyLongshot,
                    RowId::ToolRect,
                    RowId::ToolArrow,
                    RowId::ToolText,
                    RowId::ToolBlur,
                    RowId::ToolHighlight,
                    RowId::ToolGlow,
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
        assert_eq!(caps, 10, "快捷键页应有 10 个键帽行");
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
