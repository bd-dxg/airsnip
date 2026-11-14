// SPDX-License-Identifier: GPL-3.0-only
//! 候选框绘制（CAP-6 / ADR 0037）：只有一条强调色细边。
//! 手柄与工具栏留给「确认选中」之后——预览态画上手柄，用户会以为已经选好了。

use tiny_skia::{Paint, PathBuilder, Rect, Stroke, Transform};

use super::super::geometry::SelRect;
use super::super::shape::ACCENT;
use super::super::ui::Ui;

/// 画候选窗口边框。
pub(in crate::overlay) fn draw_candidate(pixmap: &mut tiny_skia::Pixmap, rect: SelRect, ui: Ui) {
    let line = ui.f(2.0);
    let Some(r) = Rect::from_xywh(
        rect.x as f32 + 0.5,
        rect.y as f32 + 0.5,
        (rect.w as f32 - 1.0).max(1.0),
        (rect.h as f32 - 1.0).max(1.0),
    ) else {
        return;
    };
    let mut pb = PathBuilder::new();
    pb.push_rect(r);
    let Some(path) = pb.finish() else {
        return;
    };
    let mut paint = Paint::default();
    paint.set_color_rgba8(ACCENT[0], ACCENT[1], ACCENT[2], 255);
    let stroke = Stroke {
        width: line,
        ..Default::default()
    };
    pixmap.stroke_path(&path, &paint, &stroke, Transform::identity(), None);
}
