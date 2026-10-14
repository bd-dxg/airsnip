// SPDX-License-Identifier: GPL-3.0-only
//! 字体列表面板（EDT-4）：工具栏第二行字体按钮展开的自绘浮层。
//! 每行用该字体自身渲染（所见即所选），当前字体高亮；滚轮滚动。
//! 布局/命中/滚动在本模块，绘制见 `paint`。

mod paint;
pub(in crate::overlay) use paint::render;

use crate::overlay::geometry::{SelRect, in_rect};
use crate::render::fonts;

/// 行高。
pub const ROW_H: i32 = 22;
/// 面板宽度。
const W: i32 = 200;
/// 面板最大高度（超出部分靠滚动）。
const MAX_H: i32 = 240;
/// 上下内边距。
const PAD: i32 = 4;
/// 滚轮一次滚动的行数。
const SCROLL_ROWS: usize = 3;

/// 打开中的字体面板状态。
#[derive(Clone)]
pub struct FontPanel {
    pub rect: SelRect,
    /// 首个可见字体在 `fonts::families()` 中的下标。
    top: usize,
    /// 鼠标悬停的字体下标。
    hover: Option<usize>,
}

impl FontPanel {
    /// 从字体按钮与工具栏位置展开：优先显示在工具栏上方，空间不足时翻到下方；
    /// 并把当前字体滚到可见区中部。
    pub fn open(btn: SelRect, bar: SelRect, current: &str, screen_w: i32, screen_h: i32) -> Self {
        let list = fonts::families();
        // 高度按整行取整（不留底部空白死区，命中/绘制严格对齐）
        let max_rows = ((MAX_H - PAD * 2) / ROW_H).max(1) as usize;
        let content = list.len().min(max_rows) as i32 * ROW_H + PAD * 2;
        let x = btn.x.clamp(4, (screen_w - W - 4).max(4));
        let above = bar.y - content - 2;
        let y = if above >= 4 {
            above
        } else {
            (bar.y + bar.h + 2).min((screen_h - content - 4).max(4))
        };
        let mut panel = Self {
            rect: SelRect { x, y, w: W, h: content },
            top: 0,
            hover: None,
        };
        let index = list.iter().position(|f| f == current).unwrap_or(0);
        panel.top = index.saturating_sub(panel.rows() / 2);
        panel.clamp_top();
        panel
    }

    /// 可见行数。
    pub fn rows(&self) -> usize {
        ((self.rect.h - PAD * 2).max(ROW_H) / ROW_H) as usize
    }

    /// 最大首行下标（列表尾部对齐）。
    fn max_top(&self) -> usize {
        fonts::families().len().saturating_sub(self.rows())
    }

    fn clamp_top(&mut self) {
        self.top = self.top.min(self.max_top());
    }

    /// 第 `row` 个可见行：字体下标、族名与行矩形；超出列表返回 None。
    pub fn row(&self, row: usize) -> Option<(usize, &'static str, SelRect)> {
        let index = self.top + row;
        let name = fonts::families().get(index)?;
        Some((index, name.as_str(), self.row_rect(row)))
    }

    fn row_rect(&self, row: usize) -> SelRect {
        SelRect {
            x: self.rect.x + 1,
            y: self.rect.y + PAD + row as i32 * ROW_H,
            w: self.rect.w - 2,
            h: ROW_H,
        }
    }

    /// 命中行：返回字体下标（面板外或空白处返回 None）。
    pub fn hit(&self, x: i32, y: i32) -> Option<usize> {
        if !in_rect(self.rect, x, y) {
            return None;
        }
        let row = ((y - self.rect.y - PAD).max(0) / ROW_H) as usize;
        // 不用 `row()` 的越界保护：面板内但落在已绘制行之外的空白也要返回 None
        if row >= self.rows() {
            return None;
        }
        self.row(row).map(|(i, _, _)| i)
    }

    /// 滚轮滚动（`delta` 为 ±120 倍数，向上为正向内容上移）；返回是否变化。
    pub fn scroll(&mut self, delta: i16) -> bool {
        let step = SCROLL_ROWS;
        let before = self.top;
        let dir = if delta > 0 { -1 } else { 1 };
        let next = self.top as i32 + dir * step as i32;
        self.top = next.clamp(0, self.max_top() as i32) as usize;
        self.top != before
    }

    /// 更新悬停行；返回是否变化（供局部重绘）。
    pub fn hover_at(&mut self, x: i32, y: i32) -> bool {
        let hover = self.hit(x, y);
        let changed = hover != self.hover;
        self.hover = hover;
        changed
    }

    /// 鼠标是否在面板内（用于滚轮与点击的分流）。
    pub fn contains(&self, x: i32, y: i32) -> bool {
        in_rect(self.rect, x, y)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn btn(x: i32, y: i32) -> SelRect {
        SelRect { x, y, w: 132, h: 26 }
    }

    /// 工具栏整体（两行）：第二行底边在 y + 64。
    fn bar(x: i32, y: i32) -> SelRect {
        SelRect { x, y, w: 560, h: 64 }
    }

    #[test]
    fn panel_stays_inside_screen_and_prefers_above() {
        let (sw, sh) = (1366, 768);
        let above = FontPanel::open(
            btn(200, 700 + 34),
            bar(200, 700),
            crate::editor::DEFAULT_FONT_FAMILY,
            sw,
            sh,
        );
        assert!(
            above.rect.y + above.rect.h <= 700,
            "应展开在工具栏上方：{:?}",
            above.rect
        );
        assert!(above.rect.x >= 0 && above.rect.x + above.rect.w <= sw);
        assert!(above.rect.y >= 0 && above.rect.y + above.rect.h <= sh);

        // 工具栏贴近顶部：翻到下方，且不与工具栏重叠
        let below = FontPanel::open(btn(200, 34), bar(200, 0), crate::editor::DEFAULT_FONT_FAMILY, sw, sh);
        assert!(below.rect.y >= 64, "应展开在工具栏下方：{:?}", below.rect);
        assert!(below.rect.y + below.rect.h <= sh);
    }

    #[test]
    fn hit_maps_row_to_font_index() {
        let panel = FontPanel::open(
            btn(200, 400 + 34),
            bar(200, 400),
            crate::editor::DEFAULT_FONT_FAMILY,
            1366,
            768,
        );
        let (index, _, r) = panel.row(0).expect("至少一行");
        assert_eq!(panel.hit(r.x + 4, r.y + 4), Some(index));
        assert_eq!(panel.hit(panel.rect.x - 10, panel.rect.y + 4), None, "面板外不命中");
        assert_eq!(
            panel.hit(panel.rect.x + 4, panel.rect.y + panel.rect.h - 1),
            None,
            "最后一行下方的空白不应命中字体"
        );
    }

    #[test]
    fn current_font_is_visible_after_open() {
        let list = fonts::families();
        let last = list.last().expect("至少一个字体");
        let panel = FontPanel::open(btn(200, 434), bar(200, 400), last, 1366, 768);
        let visible = (0..panel.rows())
            .filter_map(|r| panel.row(r))
            .any(|(_, n, _)| n == last);
        assert!(visible, "当前字体应滚入可见区（列表 {last}）");
    }

    #[test]
    fn scroll_clamps_to_list_bounds() {
        let mut panel = FontPanel::open(
            btn(200, 434),
            bar(200, 400),
            crate::editor::DEFAULT_FONT_FAMILY,
            1366,
            768,
        );
        for _ in 0..200 {
            panel.scroll(120);
        }
        assert_eq!(panel.top, 0, "上滚到底应停在首行");
        for _ in 0..200 {
            panel.scroll(-120);
        }
        assert_eq!(panel.top, panel.max_top(), "下滚到底应停在末行");
    }
}
