// SPDX-License-Identifier: GPL-3.0-only
//! 设置面板自绘（M3）：tiny-skia 绘制控件，文字经 DirectWrite 栅格化（PRD §6.4）。
//! 布局：左侧分类导航 + 右侧内容区。

use tiny_skia::{Pixmap, Rect, Transform};

use crate::editor::Point;
use crate::i18n::SettingsKey as K;
use crate::overlay::geometry::SelRect;
use crate::overlay::shape::{paint_of, rounded_rect};
use crate::render::text as dwrite;

use super::Panel;
use super::controls::{Item, Layout, NAV_W, RowId, RowKind, Section};

/// 绘制整个面板：背景 → 页签容器 → 导航栏 → 节标题 → 内容行。
pub(super) fn render(pixmap: &mut Pixmap, panel: &Panel, layout: &Layout) {
    let p = panel.palette;
    pixmap.fill(tiny_skia::Color::from_rgba8(p.bg[0], p.bg[1], p.bg[2], 255));
    // 快捷键页子页签的容器（ADR 0041）：**必须先于** `Tab` 项画，否则这层不透明的
    // 底会把页签文字盖掉（曾经就错在这里：标签看不见）。
    if let Some(bar) = layout.tab_bar {
        rounded_rect(
            pixmap,
            bar,
            (8.0 * panel.scale) as f32,
            [p.bg[0], p.bg[1], p.bg[2], 255],
            Some(p.border),
        );
    }
    let mut nav_idx = 0usize;
    let mut row_idx = 0usize;
    for item in &layout.items {
        match item {
            Item::Nav(sec) => {
                draw_nav_item(pixmap, panel, *sec, nav_idx);
                nav_idx += 1;
            }
            Item::Tab(tab) => draw_tab(pixmap, panel, tab),
            Item::Row(row) => {
                draw_row(pixmap, panel, row, row_idx);
                row_idx += 1;
            }
        }
    }
    draw_content_header(pixmap, panel);
    // 「变量说明」浮层（OUT-2 / ADR 0041）：画在所有行之上，避免被后面的行盖掉
    super::savepage::draw_help(pixmap, panel);
    // 展开的下拉列表（OUT-2 / ADR 0041）：比行更晚画，才能盖住下面的内容
    super::savepage::draw_list(pixmap, panel, layout);
    // 「工具栏」页脚注（文件组全隐时的提醒，EDT-16 R7）
    super::toolbar_page::draw_hint(pixmap, panel, layout);
    // 导航与内容分隔线
    let h = layout.height;
    let nav_w = (NAV_W as f32 * panel.scale) as f32;
    if let Some(rect) = Rect::from_xywh(nav_w - 1.0, 0.0, 1.0, h as f32) {
        pixmap.fill_rect(rect, &paint_of(p.divider), Transform::identity(), None);
    }
    // 「我要反馈」轻通知（SYS-8）：画在最上层
    super::copyinfo::draw_toast(pixmap, panel, layout.height);
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

/// 快捷键页的子页签（ADR 0041）：分段控件——未选中透明（只靠外框让人知道这排可点），
/// 选中项内缩成一个强调色胶囊，悬停项给浅底。三个页签等宽，因此选中态不会让文字跳位。
fn draw_tab(pixmap: &mut Pixmap, panel: &Panel, tab: &super::controls::Tab) {
    let p = panel.palette;
    let s = panel.scale;
    let selected = panel.hotkey_group == tab.group;
    let hovered = panel.tab_hover == Some(tab.group);
    let inset = (super::controls::TAB_INSET as f32 * s) as i32;
    let pill = SelRect {
        x: tab.rect.x + inset,
        y: tab.rect.y + inset,
        w: tab.rect.w - inset * 2,
        h: tab.rect.h - inset * 2,
    };
    if selected {
        rounded_rect(pixmap, pill, (6.0 * s) as f32, p.accent_soft, Some(p.accent));
    } else if hovered {
        rounded_rect(pixmap, pill, (6.0 * s) as f32, p.row_hover, None);
    }
    let size = 15.0 * s;
    let title = tab.group.title();
    let (tw, _) = dwrite::measure(title, &dwrite::FontSpec::new(size));
    let x = tab.rect.x as f32 + (tab.rect.w as f32 - tw) / 2.0;
    let y = tab.rect.y as f32 + (tab.rect.h as f32 - size * 1.2) / 2.0;
    let color = if selected {
        p.accent
    } else if hovered {
        p.text
    } else {
        p.text_dim
    };
    draw_text(pixmap, x, y, title, size, color);
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
    // 按钮行不高亮整行（否则是一条长条），改由按钮自身高亮（见 `link::draw_button`）；
    // 「关于」行亦然——整行只有一个「我要反馈」按钮可点。
    if hovered && !matches!(row.kind, RowKind::Button | RowKind::About) {
        rounded_rect(pixmap, row.hover, (6.0 * s) as f32, p.row_hover, None);
    }
    let size = 16.0 * s;
    let lx = row.rect.x as f32 + row.label_pad as f32;
    // 支持项目块与组头的标签由各自的绘制函数负责，此处不重复绘制
    if !matches!(row.kind, RowKind::Qr | RowKind::Group) {
        // 快捷键单元格（ADR 0041）：标签左、键帽右并排，垂直居中
        let ly = row.rect.y as f32 + (row.rect.h as f32 - size * 1.2) / 2.0;
        // 开关行文案跟状态走（ADR 0036）：关闭态写默认行为，开启态写被打开的那个行为，
        // 否则用户看不出「开」到底是哪个含义。
        let label = toggle_label(row.id, &panel.config).unwrap_or(row.label);
        draw_text(pixmap, lx, ly, label, size, p.text);
    }
    match row.kind {
        RowKind::Toggle => super::widgets::draw_toggle(pixmap, panel, row),
        RowKind::Group => super::toolbar_page::draw_group(pixmap, panel, row, hovered),
        RowKind::KeyCap => super::keycap::draw(pixmap, panel, row, hovered),
        RowKind::Radio => super::widgets::draw_radio(pixmap, panel, row),
        RowKind::Readonly => super::widgets::draw_readonly(pixmap, panel, row),
        RowKind::About => draw_about(pixmap, panel, row, hovered),
        RowKind::Link => super::link::draw_link(pixmap, panel, row),
        RowKind::Button => super::link::draw_button(pixmap, panel, row, hovered),
        RowKind::Qr => super::reward::draw_block(pixmap, panel, row),
        // OUT-2 / ADR 0041：路径模板输入框、格式 / 同名下拉、预览行
        RowKind::TextInput => super::textbox::draw(pixmap, panel),
        RowKind::Select => super::savepage::draw(pixmap, panel, row),
    }
}

/// 开关行的标签随状态变化（ADR 0036）：两行开关的「开」和「关」各是一种具体行为，
/// 只写一半（如固定写「…·贴到屏幕」）用户不知道关掉是什么意思。
/// 返回 `None` 表示该行文案是静态的（用 `Row::label`）。
pub(super) fn toggle_label(id: RowId, cfg: &crate::settings::Config) -> Option<&'static str> {
    match id {
        RowId::PinPosCenter => Some(match cfg.pin_position {
            crate::settings::PinPosition::Center => K::GeneralPinCenter.text(),
            crate::settings::PinPosition::Original => K::GeneralPinOriginal.text(),
        }),
        RowId::DoubleClick => Some(match cfg.double_click_action {
            crate::settings::DoubleClickAction::Pin => K::GeneralDoubleClickPin.text(),
            crate::settings::DoubleClickAction::Copy => K::GeneralDoubleClickCopy.text(),
        }),
        _ => None,
    }
}

/// 关于行：右侧版本与许可，再右侧是「我要反馈」按钮（SYS-8）。
fn draw_about(pixmap: &mut Pixmap, panel: &Panel, row: &super::controls::Row, hovered: bool) {
    let text = format!("v{} · GPL-3.0", env!("CARGO_PKG_VERSION"));
    let btn = super::link::about_button_rect(panel, row);
    let size = 16.0 * panel.scale;
    let (w, _) = dwrite::measure(&text, &dwrite::FontSpec::new(size));
    // 版本文字右对齐到按钮左缘前，给按钮留 12px 间距
    let x = btn.x as f32 - 12.0 * panel.scale - w;
    let y = row.rect.y as f32 + (row.rect.h as f32 - size * 1.2) / 2.0;
    draw_text(pixmap, x, y, &text, size, panel.palette.text_dim);
    super::link::draw_about_button(pixmap, panel, row, hovered);
}

/// 右侧对齐的说明文本（只读行 / 关于行共用）。
pub(super) fn draw_right_text(pixmap: &mut Pixmap, panel: &Panel, row: &super::controls::Row, text: &str) {
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
