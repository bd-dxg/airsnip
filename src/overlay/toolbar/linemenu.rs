// SPDX-License-Identifier: GPL-3.0-only
//! 线型下拉（EDT-18，issue #168）：工具栏「线型」格子弹开的自绘浮层。
//! 三行各画一段真实线型样本（下拉项**只要缩略图、不加文字**，见 ADR 0042），
//! 当前项用强调底色标出，悬停项给底色。展开方向与取值语义同字体列表（`fontlist`）：
//! 优先在工具栏上方，空间不足翻到下方并钳回屏内。

use tiny_skia::Pixmap;

use crate::editor::LineStyle;
use crate::overlay::geometry::{SelRect, in_rect};
use crate::overlay::shape::rounded_rect;
use crate::overlay::ui::Ui;
use crate::theme::Palette;

use super::line;

/// 行高基准值。
const ROW_H_BASE: f32 = 26.0;
/// 面板宽度基准值（够放下点划线的一个完整周期）。
const W_BASE: f32 = 96.0;
/// 上下内边距基准值。
const PAD_BASE: f32 = 4.0;

/// 展开中的线型下拉。
#[derive(Clone, Debug)]
pub struct LineMenu {
    pub rect: SelRect,
    /// 界面缩放（行高、样本粗细与圆角随之缩放）。
    ui: Ui,
    /// 鼠标悬停的项（`LineStyle::ALL` 的下标）。
    hover: Option<usize>,
}

impl LineMenu {
    /// 从线型格子与工具栏位置展开：优先显示在工具栏上方，空间不足时翻到下方。
    pub fn open(btn: SelRect, bar: SelRect, vw: i32, vh: i32, ui: Ui) -> Self {
        let (row_h, w, pad) = (ui.px(ROW_H_BASE), ui.px(W_BASE), ui.px(PAD_BASE));
        let h = row_h * LineStyle::ALL.len() as i32 + pad * 2;
        let margin = ui.px(4.0);
        let gap = ui.px(2.0);
        let x = btn.x.clamp(margin, (vw - w - margin).max(margin));
        let above = bar.y - h - gap;
        let y = if above >= margin {
            above
        } else {
            (bar.y + bar.h + gap).min((vh - h - margin).max(margin))
        };
        Self {
            rect: SelRect { x, y, w, h },
            ui,
            hover: None,
        }
    }

    /// 界面缩放（绘制用）。
    pub(in crate::overlay) fn ui(&self) -> Ui {
        self.ui
    }

    fn row_h(&self) -> i32 {
        self.ui.px(ROW_H_BASE)
    }

    fn pad(&self) -> i32 {
        self.ui.px(PAD_BASE)
    }

    /// 某一项的行矩形（绘制与命中共用一份几何）。
    fn row_rect(&self, index: usize) -> SelRect {
        SelRect {
            x: self.rect.x + 1,
            y: self.rect.y + self.pad() + index as i32 * self.row_h(),
            w: self.rect.w - 2,
            h: self.row_h(),
        }
    }

    /// 命中项：返回线型（面板外或空白处返回 None）。
    pub fn hit(&self, x: i32, y: i32) -> Option<LineStyle> {
        if !in_rect(self.rect, x, y) {
            return None;
        }
        let row = ((y - self.rect.y - self.pad()).max(0) / self.row_h()) as usize;
        LineStyle::ALL.get(row).copied()
    }

    /// 更新悬停项；返回是否变化（供局部重绘）。
    pub fn hover_at(&mut self, x: i32, y: i32) -> bool {
        let hover = self.hit(x, y).and_then(|s| LineStyle::ALL.iter().position(|i| *i == s));
        let changed = hover != self.hover;
        self.hover = hover;
        changed
    }

    /// 鼠标是否在面板内（光标形状与点击分流用）。
    pub fn contains(&self, x: i32, y: i32) -> bool {
        in_rect(self.rect, x, y)
    }
}

/// 绘制下拉：底板 + 三项样本；当前项填强调底色，悬停项填悬停底色。
pub(in crate::overlay) fn render(pixmap: &mut Pixmap, menu: &LineMenu, current: LineStyle, p: &Palette) {
    let ui = menu.ui();
    rounded_rect(pixmap, menu.rect, ui.f(7.0), p.bg, Some(p.border));
    for (i, style) in LineStyle::ALL.iter().enumerate() {
        let r = menu.row_rect(i);
        let active = *style == current;
        let hovered = menu.hover == Some(i);
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

    /// 展开方向：工具栏在上半屏时向下翻，贴底时向上翻，且不越出屏幕。
    #[test]
    fn menu_stays_inside_screen() {
        let (vw, vh) = (1366, 768);
        let top = LineMenu::open(btn(200, 34), bar(200, 0), vw, vh, base());
        assert!(top.rect.y >= 34, "工具栏贴顶时应翻到下方：{:?}", top.rect);
        let bottom = LineMenu::open(btn(200, 700), bar(200, 700), vw, vh, base());
        assert!(bottom.rect.y + bottom.rect.h <= 700, "工具栏贴底时应展开在上方");
        for m in [top, bottom] {
            assert!(m.rect.x >= 0 && m.rect.x + m.rect.w <= vw);
            assert!(m.rect.y >= 0 && m.rect.y + m.rect.h <= vh);
        }
    }

    /// 三行各命中自己，行外空白与面板外不命中。
    #[test]
    fn hit_maps_rows_to_styles() {
        let m = LineMenu::open(btn(200, 400 + 34), bar(200, 400), 1366, 768, base());
        for (i, style) in LineStyle::ALL.iter().enumerate() {
            let r = m.row_rect(i);
            assert_eq!(m.hit(r.x + 4, r.y + 4), Some(*style), "第 {i} 行应命中 {style:?}");
        }
        assert_eq!(m.hit(m.rect.x - 10, m.rect.y + 4), None, "面板外不命中");
        assert_eq!(
            m.hit(m.rect.x + 4, m.rect.y + m.rect.h - 1),
            None,
            "最后一行下方的空白不应命中"
        );
    }

    /// 高 DPI 下面板整体放大（CAP-5）。
    #[test]
    fn menu_scales_with_dpi() {
        let a = LineMenu::open(btn(200, 434), bar(200, 400), 1366, 768, base());
        let b = LineMenu::open(btn(400, 868), bar(400, 800), 2732, 1536, Ui::from_dpi(192));
        assert_eq!(b.rect.w, a.rect.w * 2, "面板宽度应翻倍");
        assert_eq!(b.rect.h, a.rect.h * 2, "面板高度应翻倍");
    }
}
