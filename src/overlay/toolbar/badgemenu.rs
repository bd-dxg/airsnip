// SPDX-License-Identifier: GPL-3.0-only
//! 徽标形态下拉（EDT-20，issue #186）：工具栏「序号徽标」格子弹开的自绘浮层。
//! 三行各画一个徽标缩略图（数字 / 对勾 / 叉号），当前项用强调底色标出，悬停项给底色。
//! 布局与命中走 `menu`（与线型下拉共用）。

use tiny_skia::Pixmap;

use crate::editor::{BadgeMark, Point};
use crate::overlay::geometry::SelRect;
use crate::overlay::shape::rounded_rect;
use crate::overlay::ui::Ui;
use crate::render;
use crate::theme::Palette;

use super::menu::Menu;

/// 面板宽度基准值（够放下徽标缩略图与两侧留白）。
const W_BASE: f32 = 72.0;

/// 展开中的徽标形态下拉。
#[derive(Clone, Debug)]
pub struct BadgeMenu {
    menu: Menu,
}

impl BadgeMenu {
    /// 从徽标格子与工具栏位置展开：优先显示在工具栏上方，空间不足时翻到下方。
    pub fn open(btn: SelRect, bar: SelRect, vw: i32, vh: i32, ui: Ui) -> Self {
        Self {
            menu: Menu::open(btn, bar, BadgeMark::ALL.len(), W_BASE, vw, vh, ui),
        }
    }

    /// 界面缩放（绘制用）。
    pub(in crate::overlay) fn ui(&self) -> Ui {
        self.menu.ui()
    }

    /// 面板矩形（脏区用）。
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

    /// 命中项：返回形态（面板外或空白处返回 None）。
    pub fn hit(&self, x: i32, y: i32) -> Option<BadgeMark> {
        self.menu.hit_row(x, y).and_then(|i| BadgeMark::ALL.get(i).copied())
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

/// 绘制下拉：底板 + 三项徽标缩略图；当前项填强调底色，悬停项填悬停底色。
pub(in crate::overlay) fn render(pixmap: &mut Pixmap, menu: &BadgeMenu, current: BadgeMark, p: &Palette) {
    let ui = menu.ui();
    rounded_rect(pixmap, menu.menu.rect, ui.f(7.0), p.bg, Some(p.border));
    for (i, mark) in BadgeMark::ALL.iter().enumerate() {
        let r = menu.row_rect(i);
        let active = *mark == current;
        if active || menu.hovered(i) {
            let fill = if active { p.accent_soft } else { p.row_hover };
            rounded_rect(pixmap, r, ui.f(5.0), fill, None);
        }
        // 缩略图：半径取行高的一部分，内容色按底色对比度取（与画面上的徽标同一套规则）
        let radius = r.h as f32 * 0.36;
        let center = Point::new(r.x as f32 + r.w as f32 * 0.5, r.y as f32 + r.h as f32 * 0.5);
        let body = if active { p.accent } else { p.icon };
        render::badge_preview(pixmap, center, radius, mark.sample(), body, render::contrast_ink(body));
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

    /// 三行各命中自己，面板外不命中（几何细节见 `menu` 的测试）。
    #[test]
    fn hit_maps_rows_to_marks() {
        let m = BadgeMenu::open(btn(200, 400 + 34), bar(200, 400), 1366, 768, base());
        for (i, mark) in BadgeMark::ALL.iter().enumerate() {
            let r = m.row_rect(i);
            assert_eq!(m.hit(r.x + 4, r.y + 4), Some(*mark), "第 {i} 行应命中 {mark:?}");
        }
        assert_eq!(m.hit(m.rect().x - 10, m.rect().y + 4), None, "面板外不命中");
    }

    /// 三行都画出了墨水（不是空面板）：徽标缩略图可见。
    #[test]
    fn render_paints_marks() {
        let m = BadgeMenu::open(btn(200, 434), bar(200, 400), 1366, 768, base());
        // 面板按**屏幕坐标**绘制（与真实绘制一致），所以画布要包住它的原点
        let mut px = Pixmap::new((m.rect().x + m.rect().w) as u32, (m.rect().y + m.rect().h) as u32).unwrap();
        render(&mut px, &m, BadgeMark::Check, &crate::theme::LIGHT);
        for (i, _) in BadgeMark::ALL.iter().enumerate() {
            let r = m.row_rect(i);
            let cx = (r.x + r.w / 2) as u32;
            let cy = (r.y + r.h / 2) as u32;
            let px_ = px.pixel(cx, cy).expect("行中心应有像素");
            assert!(px_.alpha() > 0, "第 {i} 行中心应画出徽标");
        }
    }
}
