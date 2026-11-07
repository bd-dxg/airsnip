// SPDX-License-Identifier: GPL-3.0-only
//! 文本输入框绘制（EDT-4）：文字 + 带描边的光标 + 组合串下划线。
//! 从 `textinput` 拆出，控制单文件规模；输入状态与字符逻辑见 `super` 与 `text_edit_state`。

use tiny_skia::Pixmap;

use crate::overlay::text_edit_state::TextEdit;

/// 输入框绘制（selection::repaint 调用于对象之上）：文字 + 带描边的光标 + 组合串下划线。
pub(in crate::overlay) fn draw_edit(pixmap: &mut Pixmap, edit: &TextEdit, color: [u8; 4]) {
    let pad = crate::editor::text::pad(edit.font_size);

    // 文本（已确认 + 组合串）
    crate::render::text::draw(
        pixmap,
        edit.pos,
        &edit.display(),
        &edit.spec(),
        color,
        tiny_skia::Transform::identity(),
    );

    // 光标（竖线，白底 + 黑描边，任意底色下可见）；多行时光标按行定位
    let cx = (edit.pos.x + edit.caret_x()).round();
    let cy = edit.pos.y + edit.caret_row() as f32 * edit.line_height() + pad * 0.4;
    if let Some(rect) = tiny_skia::Rect::from_xywh(cx, cy, 2.0, edit.font_size * 1.1)
        && let Some(path) = caret_path(rect)
    {
        let mut black = tiny_skia::Paint::default();
        black.set_color_rgba8(0, 0, 0, 255);
        let edge = tiny_skia::Stroke {
            width: 1.0,
            ..Default::default()
        };
        pixmap.stroke_path(&path, &black, &edge, tiny_skia::Transform::identity(), None);
        let mut white = tiny_skia::Paint::default();
        white.set_color_rgba8(255, 255, 255, 255);
        pixmap.fill_path(
            &path,
            &white,
            tiny_skia::FillRule::Winding,
            tiny_skia::Transform::identity(),
            None,
        );
    }

    // 组合串下划线（IME 候选标记，贴在光标行文本下方）
    if !edit.comp.is_empty() {
        let (cw, _) = crate::render::text::measure(&edit.comp.iter().collect::<String>(), &edit.spec());
        let x0 = edit.pos.x + edit.caret_x() - cw;
        let y = edit.pos.y + (edit.caret_row() + 1) as f32 * edit.line_height() + pad * 0.3;
        if let Some(rect) = tiny_skia::Rect::from_xywh(x0, y, cw.max(1.0), 1.0) {
            let mut pb = tiny_skia::PathBuilder::new();
            pb.push_rect(rect);
            if let Some(path) = pb.finish() {
                let mut paint = tiny_skia::Paint::default();
                paint.set_color_rgba8(255, 255, 255, 180);
                pixmap.fill_path(
                    &path,
                    &paint,
                    tiny_skia::FillRule::Winding,
                    tiny_skia::Transform::identity(),
                    None,
                );
            }
        }
    }
}

/// 光标竖线路径（四周外扩 1px 供描边）。
fn caret_path(rect: tiny_skia::Rect) -> Option<tiny_skia::Path> {
    let mut pb = tiny_skia::PathBuilder::new();
    pb.push_rect(tiny_skia::Rect::from_ltrb(
        rect.left() - 1.0,
        rect.top() - 1.0,
        rect.right() + 1.0,
        rect.bottom() + 1.0,
    )?);
    pb.finish()
}
