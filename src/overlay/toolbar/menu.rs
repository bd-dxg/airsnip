// SPDX-License-Identifier: GPL-3.0-only
//! 工具栏下拉浮层的公共几何（EDT-18 线型 / EDT-20 徽标形态）。
//! 两处下拉的**取值**不同、**几何完全一致**（行高、内边距、贴边翻转、命中、悬停），
//! 所以从 `linemenu` 抽出这一份，绘制各自实现（`linemenu::render` / `badgemenu::render`）。

use crate::overlay::geometry::{SelRect, in_rect};
use crate::overlay::ui::Ui;

/// 行高基准值。
pub(super) const ROW_H_BASE: f32 = 26.0;
/// 上下内边距基准值。
pub(super) const PAD_BASE: f32 = 4.0;

/// 一个展开中的下拉面板：几何 + 悬停项。
#[derive(Clone, Debug)]
pub struct Menu {
    pub rect: SelRect,
    ui: Ui,
    /// 项数（行数）。
    rows: usize,
    /// 鼠标悬停的项下标。
    hover: Option<usize>,
}

impl Menu {
    /// 从格子与工具栏位置展开：优先显示在工具栏上方，空间不足时翻到下方并钳回屏内。
    /// `w_base` 是面板宽度（基准像素），`rows` 是项数。
    pub fn open(btn: SelRect, bar: SelRect, rows: usize, w_base: f32, vw: i32, vh: i32, ui: Ui) -> Self {
        let (row_h, w, pad) = (ui.px(ROW_H_BASE), ui.px(w_base), ui.px(PAD_BASE));
        let h = row_h * rows as i32 + pad * 2;
        let margin = ui.px(4.0);
        let gap = ui.px(2.0);
        // 横向对着格子居中，越界时钳回屏内（格子可能贴在屏幕边缘）
        let centered = btn.x + btn.w / 2 - w / 2;
        let x = centered.clamp(margin, (vw - w - margin).max(margin));
        let above = bar.y - h - gap;
        let y = if above >= margin {
            above
        } else {
            (bar.y + bar.h + gap).min((vh - h - margin).max(margin))
        };
        Self {
            rect: SelRect { x, y, w, h },
            ui,
            rows,
            hover: None,
        }
    }

    /// 界面缩放（绘制用）。
    pub fn ui(&self) -> Ui {
        self.ui
    }

    fn row_h(&self) -> i32 {
        self.ui.px(ROW_H_BASE)
    }

    fn pad(&self) -> i32 {
        self.ui.px(PAD_BASE)
    }

    /// 某一项的行矩形（绘制与命中共用一份几何）。
    pub fn row_rect(&self, index: usize) -> SelRect {
        SelRect {
            x: self.rect.x + 1,
            y: self.rect.y + self.pad() + index as i32 * self.row_h(),
            w: self.rect.w - 2,
            h: self.row_h(),
        }
    }

    /// 命中项下标：面板外或行间空白返回 None。
    pub fn hit_row(&self, x: i32, y: i32) -> Option<usize> {
        if !in_rect(self.rect, x, y) {
            return None;
        }
        let row = ((y - self.rect.y - self.pad()).max(0) / self.row_h()) as usize;
        (row < self.rows).then_some(row)
    }

    /// 更新悬停项；返回是否变化（供局部重绘）。
    pub fn hover_at(&mut self, x: i32, y: i32) -> bool {
        let hover = self.hit_row(x, y);
        let changed = hover != self.hover;
        self.hover = hover;
        changed
    }

    /// 当前悬停项（绘制高亮用）。
    pub fn hover(&self) -> Option<usize> {
        self.hover
    }

    /// 鼠标是否在面板内（光标形状与点击分流用）。
    pub fn contains(&self, x: i32, y: i32) -> bool {
        in_rect(self.rect, x, y)
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

    /// 展开方向：工具栏在上半屏时向下翻，贴底时向上翻；格子贴屏幕边缘时面板钳回屏内。
    #[test]
    fn menu_stays_inside_screen() {
        let (vw, vh) = (1366, 768);
        let top = Menu::open(btn(200, 34), bar(200, 0), 3, 96.0, vw, vh, base());
        assert!(top.rect.y >= 34, "工具栏贴顶时应翻到下方：{:?}", top.rect);
        let bottom = Menu::open(btn(200, 700), bar(200, 700), 3, 96.0, vw, vh, base());
        assert!(bottom.rect.y + bottom.rect.h <= 700, "工具栏贴底时应展开在上方");
        let edge = Menu::open(btn(vw - 30, 400), bar(200, 400), 3, 96.0, vw, vh, base());
        assert!(edge.rect.x + edge.rect.w <= vw, "面板应钳回屏内：{:?}", edge.rect);
        for m in [top, bottom, edge] {
            assert!(m.rect.x >= 0 && m.rect.x + m.rect.w <= vw);
            assert!(m.rect.y >= 0 && m.rect.y + m.rect.h <= vh);
        }
    }

    /// 命中按行切分：每行都命中自己，行间空白与面板外不命中；行数由 `rows` 决定。
    #[test]
    fn hit_maps_rows() {
        let m = Menu::open(btn(200, 434), bar(200, 400), 3, 96.0, 1366, 768, base());
        for i in 0..3 {
            let r = m.row_rect(i);
            assert_eq!(m.hit_row(r.x + 4, r.y + 4), Some(i), "第 {i} 行");
        }
        assert_eq!(m.hit_row(m.rect.x - 10, m.rect.y + 4), None, "面板外不命中");
        assert_eq!(
            m.hit_row(m.rect.x + 4, m.rect.y + m.rect.h - 1),
            None,
            "最后一行下方的空白不应命中"
        );
        // 只有两项时第三行位置不该命中
        let two = Menu::open(btn(200, 434), bar(200, 400), 2, 96.0, 1366, 768, base());
        let r = two.row_rect(2);
        assert_eq!(two.hit_row(r.x + 4, r.y + 4), None, "超出 rows 的行不命中");
    }

    /// 高 DPI 下面板整体放大（CAP-5）。
    #[test]
    fn menu_scales_with_dpi() {
        let a = Menu::open(btn(200, 434), bar(200, 400), 3, 96.0, 1366, 768, base());
        let b = Menu::open(btn(400, 868), bar(400, 800), 3, 96.0, 2732, 1536, Ui::from_dpi(192));
        assert_eq!(b.rect.w, a.rect.w * 2, "面板宽度应翻倍");
        assert_eq!(b.rect.h, a.rect.h * 2, "面板高度应翻倍");
    }

    /// 悬停只在换行时报告变化（局部重绘判据）。
    #[test]
    fn hover_reports_changes() {
        let mut m = Menu::open(btn(200, 434), bar(200, 400), 3, 96.0, 1366, 768, base());
        let r0 = m.row_rect(0);
        assert!(m.hover_at(r0.x + 4, r0.y + 4), "首次进入应变化");
        assert!(!m.hover_at(r0.x + 6, r0.y + 6), "同一行内移动不变化");
        let r1 = m.row_rect(1);
        assert!(m.hover_at(r1.x + 4, r1.y + 4), "换行应变化");
        assert_eq!(m.hover(), Some(1));
    }
}
