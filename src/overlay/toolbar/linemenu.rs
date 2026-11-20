// SPDX-License-Identifier: GPL-3.0-only
//! 线型下拉（EDT-18，issue #168）：工具栏「线型」格子弹开的自绘浮层。
//! 三行各画一段真实线型样本（下拉项**只要缩略图、不加文字**，见 ADR 0042），
//! 当前项用强调底色标出，悬停项给底色。布局与命中走 `menu`（与徽标形态下拉共用）。

use tiny_skia::Pixmap;

use crate::editor::LineStyle;
use crate::overlay::geometry::SelRect;
use crate::overlay::shape::rounded_rect;
use crate::overlay::ui::Ui;
use crate::theme::Palette;

use super::line;
use super::menu::Menu;

/// 面板宽度基准值（够放下点划线的一个完整周期）。
const W_BASE: f32 = 96.0;

/// 展开中的线型下拉。
#[derive(Clone, Debug)]
pub struct LineMenu {
    menu: Menu,
}

impl LineMenu {
    /// 从线型格子与工具栏位置展开：优先显示在工具栏上方，空间不足时翻到下方。
    pub fn open(btn: SelRect, bar: SelRect, vw: i32, vh: i32, ui: Ui) -> Self {
        Self {
            menu: Menu::open(btn, bar, LineStyle::ALL.len(), W_BASE, vw, vh, ui),
        }
    }

    /// 界面缩放（绘制用）。
    pub(in crate::overlay) fn ui(&self) -> Ui {
        self.menu.ui()
    }

    /// 面板矩形（脏区与互斥判断用）。
    pub(in crate::overlay) fn rect(&self) -> SelRect {
        self.menu.rect
    }

    /// 某一项的行矩形（绘制用）。
    fn row_rect(&self, index: usize) -> SelRect {
        self.menu.row_rect(index)
    }

    /// 该项是否悬停中（绘制高亮用）。
    fn hovered(&self, index: usize) -> bool {
        self.menu.hover() == Some(index)
    }

    /// 命中项：返回线型（面板外或空白处返回 None）。
    pub fn hit(&self, x: i32, y: i32) -> Option<LineStyle> {
        self.menu.hit_row(x, y).and_then(|i| LineStyle::ALL.get(i).copied())
    }

    /// 更新悬停项；返回是否变化（供局部重绘）。
    pub fn hover_at(&mut self, x: i32, y: i32) -> bool {
        self.menu.hover_at(x, y)
    }

    /// 鼠标是否在面板内（光标形状与点击分流用）。
    pub fn contains(&self, x: i32, y: i32) -> bool {
        self.menu.contains(x, y)
    }
}

/// 绘制下拉：底板 + 三项样本；当前项填强调底色，悬停项填悬停底色。
pub(in crate::overlay) fn render(pixmap: &mut Pixmap, menu: &LineMenu, current: LineStyle, p: &Palette) {
    let ui = menu.ui();
    rounded_rect(pixmap, menu.menu.rect, ui.f(7.0), p.bg, Some(p.border));
    for (i, style) in LineStyle::ALL.iter().enumerate() {
        let r = menu.row_rect(i);
        let active = *style == current;
        let hovered = menu.hovered(i);
        if active || hovered {
            let fill = if active { p.accent_soft } else { p.row_hover };
            rounded_rect(pixmap, r, ui.f(5.0), fill, None);
        }
        let color = if active { p.accent } else { p.icon };
        line::draw_sample(pixmap, r, *style, color, ui);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base() -> Ui {
        Ui::from_dpi(96)
    }

    fn btn(x: i32, y: i32) -> SelRect {
        SelRect { x, y, w: 26, h: 26 }
    }

    fn bar(x: i32, y: i32) -> SelRect {
        SelRect { x, y, w: 560, h: 34 }
    }

    /// 三行各命中自己，行外空白与面板外不命中（几何细节见 `menu` 的测试）。
    #[test]
    fn hit_maps_rows_to_styles() {
        let m = LineMenu::open(btn(200, 400 + 34), bar(200, 400), 1366, 768, base());
        for (i, style) in LineStyle::ALL.iter().enumerate() {
            let r = m.row_rect(i);
            assert_eq!(m.hit(r.x + 4, r.y + 4), Some(*style), "第 {i} 行应命中 {style:?}");
        }
        assert_eq!(m.hit(m.rect().x - 10, m.rect().y + 4), None, "面板外不命中");
    }

    /// 高 DPI 下面板整体放大（CAP-5）。
    #[test]
    fn menu_scales_with_dpi() {
        let a = LineMenu::open(btn(200, 434), bar(200, 400), 1366, 768, base());
        let b = LineMenu::open(btn(400, 868), bar(400, 800), 2732, 1536, Ui::from_dpi(192));
        assert_eq!(b.rect().w, a.rect().w * 2, "面板宽度应翻倍");
        assert_eq!(b.rect().h, a.rect().h * 2, "面板高度应翻倍");
    }
}
