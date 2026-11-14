// SPDX-License-Identifier: GPL-3.0-only
//! 设置面板自绘（M3）：tiny-skia 绘制控件，文字经 DirectWrite 栅格化（PRD §6.4）。
//! 布局：左侧分类导航 + 右侧内容区。

use tiny_skia::{Pixmap, Rect, Transform};

use crate::editor::Point;
use crate::overlay::geometry::SelRect;
use crate::overlay::shape::{paint_of, rounded_rect, rounded_rect_outline};
use crate::render::text as dwrite;
use crate::settings::ThemeSetting;

use super::Panel;
use super::controls::{Item, Layout, NAV_W, RowId, RowKind, Section};

/// 绘制整个面板：背景 → 导航栏 → 节标题 → 内容行。
pub(super) fn render(pixmap: &mut Pixmap, panel: &Panel, layout: &Layout) {
    let p = panel.palette;
    pixmap.fill(tiny_skia::Color::from_rgba8(p.bg[0], p.bg[1], p.bg[2], 255));
    let mut nav_idx = 0usize;
    let mut row_idx = 0usize;
    for item in &layout.items {
        match item {
            Item::Nav(sec) => {
                draw_nav_item(pixmap, panel, *sec, nav_idx);
                nav_idx += 1;
            }
            Item::Row(row) => {
                draw_row(pixmap, panel, row, row_idx);
                row_idx += 1;
            }
        }
    }
    draw_content_header(pixmap, panel);
    // 「工具栏」页脚注（文件组全隐时的提醒，EDT-16 R7）
    super::toolbar_page::draw_hint(pixmap, panel, layout);
    // 导航与内容分隔线
    let h = layout.height;
    let nav_w = (NAV_W as f32 * panel.scale) as f32;
    if let Some(rect) = Rect::from_xywh(nav_w - 1.0, 0.0, 1.0, h as f32) {
        pixmap.fill_rect(rect, &paint_of(p.divider), Transform::identity(), None);
    }
}

/// 左侧导航项：选中项左侧竖条 + 强调底色。
fn draw_nav_item(pixmap: &mut Pixmap, panel: &Panel, sec: Section, idx: usize) {
    let p = panel.palette;
    let s = panel.scale;
    let h = (super::controls::NAV_H as f32 * s) as i32;
    let rect = SelRect {
        x: 0,
        y: idx as i32 * h,
        w: (super::controls::NAV_W as f32 * s) as i32,
        h,
    };
    let selected = panel.section == sec;
    let hovered = panel.nav_hover == Some(sec);
    if selected {
        pixmap.fill_rect(
            Rect::from_ltrb(
                rect.x as f32,
                rect.y as f32,
                (rect.x + rect.w) as f32,
                (rect.y + rect.h) as f32,
            )
            .unwrap(),
            &paint_of(p.accent_soft),
            Transform::identity(),
            None,
        );
        // 左侧选中竖条
        if let Some(bar) = Rect::from_xywh(0.0, rect.y as f32, 3.0, rect.h as f32) {
            pixmap.fill_rect(bar, &paint_of(p.accent), Transform::identity(), None);
        }
    } else if hovered {
        pixmap.fill_rect(
            Rect::from_ltrb(
                rect.x as f32,
                rect.y as f32,
                (rect.x + rect.w) as f32,
                (rect.y + rect.h) as f32,
            )
            .unwrap(),
            &paint_of(p.row_hover),
            Transform::identity(),
            None,
        );
    }
    let size = 16.0 * s;
    let color = if selected { p.accent } else { p.text };
    let x = super::controls::PAD as f32 * 0.7 * s;
    let y = rect.y as f32 + (rect.h as f32 - size * 1.2) / 2.0;
    draw_text(pixmap, x, y, sec.title(), size, color);
}

/// 右侧内容区顶部节标题。
fn draw_content_header(pixmap: &mut Pixmap, panel: &Panel) {
    let p = panel.palette;
    let s = panel.scale;
    let size = 20.0 * s;
    let x = (super::controls::NAV_W as f32 + super::controls::PAD as f32) * s;
    draw_text(pixmap, x, (12.0 * s).max(4.0), panel.section.title(), size, p.text);
}

/// 绘制一行：悬停底色 + 标签 + 右侧控件。
fn draw_row(pixmap: &mut Pixmap, panel: &Panel, row: &super::controls::Row, idx: usize) {
    let p = panel.palette;
    let s = panel.scale;
    let hovered = panel.hover == Some(idx);
    // 悬停底色：圆角矩形，比内容外扩 `HOVER_PAD`，四周留出余量（不再紧贴控件）。
    // 按钮行不高亮整行（否则是一条长条），改由按钮自身高亮（见 `link::draw_button`）。
    if hovered && row.kind != RowKind::Button {
        rounded_rect(pixmap, row.hover, (6.0 * s) as f32, p.row_hover, None);
    }
    let size = 16.0 * s;
    let lx = row.rect.x as f32 + row.label_pad as f32;
    // 支持项目块与组头的标签由各自的绘制函数负责，此处不重复绘制
    if !matches!(row.kind, RowKind::Qr | RowKind::Group) {
        let ly = row.rect.y as f32 + (row.rect.h as f32 - size * 1.2) / 2.0;
        // 开关行文案跟状态走（ADR 0036）：关闭态写默认行为，开启态写被打开的那个行为，
        // 否则用户看不出「开」到底是哪个含义。
        let label = toggle_label(row.id, &panel.config).unwrap_or(row.label);
        draw_text(pixmap, lx, ly, label, size, p.text);
    }
    match row.kind {
        RowKind::Toggle => draw_toggle(pixmap, panel, row),
        RowKind::Group => super::toolbar_page::draw_group(pixmap, panel, row, hovered),
        RowKind::KeyCap => super::keycap::draw(pixmap, panel, row, hovered),
        RowKind::Radio => draw_radio(pixmap, panel, row),
        RowKind::Readonly => draw_readonly(pixmap, panel, row),
        RowKind::About => draw_about(pixmap, panel, row),
        RowKind::Link => super::link::draw_link(pixmap, panel, row),
        RowKind::Button => super::link::draw_button(pixmap, panel, row, hovered),
        RowKind::Qr => super::reward::draw_block(pixmap, panel, row),
    }
}

/// 开关：胶囊轨道 + 白色滑钮。状态按行类型取。
fn draw_toggle(pixmap: &mut Pixmap, panel: &Panel, row: &super::controls::Row) {
    let p = panel.palette;
    let s = panel.scale;
    let on = match row.id {
        RowId::Autostart => panel.autostart_on,
        RowId::ConfirmCancel => panel.config.confirm_cancel,
        RowId::AutoCheckUpdate => panel.config.auto_check_update,
        // 贴图位置（M2b）：开 = 屏幕中间，关 = 截图原位置
        RowId::PinPosCenter => panel.config.pin_position == crate::settings::PinPosition::Center,
        // 双击选区空白（ADR 0036）：开 = 贴图，关 = 复制
        RowId::DoubleClick => panel.config.double_click_action == crate::settings::DoubleClickAction::Pin,
        // 窗口吸附（CAP-6 / ADR 0037）：开 = 悬停预览窗口矩形，关 = 只有自由框选
        RowId::WindowSnap => panel.config.window_snap,
        // 工具栏元素显隐（EDT-16）：纯视觉开关，默认全开
        RowId::Toolbar(slot) => panel.config.toolbar.visible(slot),
        _ => false,
    };
    let c = &row.control;
    let tw = (super::controls::TOGGLE_W as f32 * s) as i32;
    let th = (18.0 * s) as i32;
    let tx = c.x + c.w - tw;
    let ty = c.y + (c.h - th) / 2;
    let track = SelRect {
        x: tx,
        y: ty,
        w: tw,
        h: th,
    };
    rounded_rect(
        pixmap,
        track,
        (th / 2) as f32,
        if on { p.toggle_on } else { p.toggle_off },
        None,
    );
    let kd = (14.0 * s) as i32;
    let kx = if on { tx + tw - kd - 2 } else { tx + 2 };
    let knob = SelRect {
        x: kx,
        y: ty + (th - kd) / 2,
        w: kd,
        h: kd,
    };
    rounded_rect(pixmap, knob, (kd / 2) as f32, [255, 255, 255, 255], None);
}

/// 开关行的标签随状态变化（ADR 0036）：两行开关的「开」和「关」各是一种具体行为，
/// 只写一半（如固定写「…·贴到屏幕」）用户不知道关掉是什么意思。
/// 返回 `None` 表示该行文案是静态的（用 `Row::label`）。
pub(super) fn toggle_label(id: RowId, cfg: &crate::settings::Config) -> Option<&'static str> {
    match id {
        RowId::PinPosCenter => Some(match cfg.pin_position {
            crate::settings::PinPosition::Center => "贴图位置·屏幕中间",
            crate::settings::PinPosition::Original => "贴图位置·截图原位置",
        }),
        RowId::DoubleClick => Some(match cfg.double_click_action {
            crate::settings::DoubleClickAction::Pin => "双击选区空白·贴到屏幕",
            crate::settings::DoubleClickAction::Copy => "双击选区空白·复制截图",
        }),
        _ => None,
    }
}

/// 主题单选：○ 外圈 + 选中时 ● 内圈。
fn draw_radio(pixmap: &mut Pixmap, panel: &Panel, row: &super::controls::Row) {
    let p = panel.palette;
    let s = panel.scale;
    let selected = match row.id {
        RowId::ThemeFollow => panel.config.theme == ThemeSetting::Follow,
        RowId::ThemeLight => panel.config.theme == ThemeSetting::Light,
        RowId::ThemeDark => panel.config.theme == ThemeSetting::Dark,
        RowId::NameDateTime => panel.config.name_style == crate::settings::NameStyle::DateTime,
        RowId::NameTimestamp => panel.config.name_style == crate::settings::NameStyle::Timestamp,
        RowId::NameDate => panel.config.name_style == crate::settings::NameStyle::Date,
        // 界面缩放（CAP-5）：选项与行 ID 的映射集中在 `controls`
        id => super::controls::ui_scale_of(id).is_some_and(|m| panel.config.ui_scale == m),
    };
    let c = &row.control;
    let d = (14.0 * s) as i32;
    let ring = SelRect {
        x: c.x + c.w - d,
        y: c.y + (c.h - d) / 2,
        w: d,
        h: d,
    };
    rounded_rect_outline(
        pixmap,
        ring,
        (d / 2) as f32,
        if selected { p.accent } else { p.text_dim },
    );
    if selected {
        let inset = (d as f32 * 0.28) as i32;
        let dot = SelRect {
            x: ring.x + inset,
            y: ring.y + inset,
            w: d - inset * 2,
            h: d - inset * 2,
        };
        rounded_rect(pixmap, dot, (dot.h / 2) as f32, p.accent, None);
    }
}

/// 只读行：右侧说明文本。
fn draw_readonly(pixmap: &mut Pixmap, panel: &Panel, row: &super::controls::Row) {
    let text = match row.id {
        RowId::Lang => "简体中文",
        RowId::Widenote => "无修饰键会覆盖其他程序按键，建议 Ctrl/Alt",
        _ => "",
    };
    draw_right_text(pixmap, panel, row, text);
}

/// 关于行：右侧版本与许可。
fn draw_about(pixmap: &mut Pixmap, panel: &Panel, row: &super::controls::Row) {
    let text = format!("v{} · GPL-3.0", env!("CARGO_PKG_VERSION"));
    draw_right_text(pixmap, panel, row, &text);
}

/// 右侧对齐的说明文本（只读行 / 关于行共用）。
fn draw_right_text(pixmap: &mut Pixmap, panel: &Panel, row: &super::controls::Row, text: &str) {
    let size = 16.0 * panel.scale;
    let (w, _) = dwrite::measure(text, &dwrite::FontSpec::new(size));
    let x = row.control.x + row.control.w as i32 - w as i32;
    let y = row.rect.y as f32 + (row.rect.h as f32 - size * 1.2) / 2.0;
    draw_text(pixmap, x as f32, y, text, size, panel.palette.text_dim);
}

/// 经 DirectWrite 栅格化文本（位置为左上角物理像素）；其他绘制模块共用。
pub(super) fn draw_text(pixmap: &mut Pixmap, x: f32, y: f32, text: &str, size: f32, color: [u8; 4]) {
    let spec = dwrite::FontSpec::new(size);
    dwrite::draw(pixmap, Point::new(x, y), text, &spec, color, Transform::identity());
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::{Config, DoubleClickAction, PinPosition};

    /// ADR 0036：行内开关的文案必须跟状态走——关闭态写默认行为、开启态写被打开的那个行为，
    /// 否则用户只能看到一半含义（「·贴到屏幕」关掉是什么？）。
    #[test]
    fn toggle_labels_follow_state() {
        let mut cfg = Config::default();
        assert_eq!(toggle_label(RowId::PinPosCenter, &cfg), Some("贴图位置·截图原位置"));
        cfg.pin_position = PinPosition::Center;
        assert_eq!(toggle_label(RowId::PinPosCenter, &cfg), Some("贴图位置·屏幕中间"));

        let mut cfg = Config::default();
        assert_eq!(toggle_label(RowId::DoubleClick, &cfg), Some("双击选区空白·复制截图"));
        cfg.double_click_action = DoubleClickAction::Pin;
        assert_eq!(toggle_label(RowId::DoubleClick, &cfg), Some("双击选区空白·贴到屏幕"));

        // 其余行（开机自启 / 连按两次退出 / 工具栏元素 / 窗口吸附 …）保持静态文案：
        // 窗口吸附只有一个含义（开 = 吸、关 = 不吸），不像「贴图位置」那样两端各是一种行为
        for id in [
            RowId::Autostart,
            RowId::ConfirmCancel,
            RowId::AutoCheckUpdate,
            RowId::WindowSnap,
        ] {
            assert_eq!(toggle_label(id, &Config::default()), None, "{id:?} 不应有动态文案");
        }
    }
}
