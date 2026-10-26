// SPDX-License-Identifier: GPL-3.0-only
//! 字体列表面板绘制（EDT-4）：每行用该字体自身渲染，当前字体高亮。

use std::sync::Arc;

use tiny_skia::Pixmap;

use crate::overlay::SelRect;
use crate::render::text::FontSpec;
use crate::theme::Palette;

use super::super::label::{LABEL_SIZE, centered_text, fit_name};
use super::FontPanel;
use crate::overlay::shape::rounded_rect;

/// 绘制面板；`current` 为当前字体族（高亮该行）。
pub(in crate::overlay) fn render(pixmap: &mut Pixmap, panel: &FontPanel, current: &str, p: &Palette) {
    rounded_rect(pixmap, panel.rect, 6.0, p.bg, Some(p.border));
    for row in 0..panel.rows() {
        let Some((index, name, r)) = panel.row(row) else {
            break;
        };
        if panel.hover == Some(index) {
            rounded_rect(pixmap, r, 4.0, p.row_hover, None);
        }
        let selected = name == current;
        if selected {
            rounded_rect(pixmap, r, 4.0, p.accent_soft, None);
        }
        // 每行用自己的字体渲染：所见即所选
        let spec = FontSpec {
            family: Arc::from(name),
            size: LABEL_SIZE + 1.0,
            bold: false,
            italic: false,
        };
        let text = fit_name(name, (r.w - 12) as f32, &spec);
        let inner = SelRect {
            x: r.x + 6,
            w: (r.w - 12).max(1),
            ..r
        };
        let color = if selected { p.accent } else { p.text };
        centered_text(pixmap, inner, &text, &spec, color);
    }
}
