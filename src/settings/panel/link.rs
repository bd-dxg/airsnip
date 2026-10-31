// SPDX-License-Identifier: GPL-3.0-only
//! 外部链接行绘制（M5）：关于页「项目地址」与「第三方许可」入口，
//! 点击用浏览器打开（见 `interact`）。从 `draw` 拆出，保证单文件不超过 300 行。

use tiny_skia::{Pixmap, Transform};

use crate::editor::Point;
use crate::render::text as dwrite;

use super::Panel;
use super::controls::RowId;

/// 外部链接行：右侧强调色文字，文案按行类型取。
pub(super) fn draw_link(pixmap: &mut Pixmap, panel: &Panel, row: &super::controls::Row) {
    let s = panel.scale;
    let text = match row.id {
        RowId::Repo => "github.com/bd-dxg/airsnip ›",
        RowId::Licenses => "查看清单 ›",
        _ => "",
    };
    let size = 16.0 * s;
    let (w, _) = dwrite::measure(text, &dwrite::FontSpec::new(size));
    let x = row.control.x + row.control.w as i32 - w as i32;
    let y = row.rect.y as f32 + (row.rect.h as f32 - size * 1.2) / 2.0;
    dwrite::draw(
        pixmap,
        Point::new(x as f32, y),
        text,
        &dwrite::FontSpec::new(size),
        panel.palette.accent,
        Transform::identity(),
    );
}
