// SPDX-License-Identifier: GPL-3.0-only
//! 右侧强调色文本行绘制（M5/SYS-2）：关于页的外部链接入口（点击用浏览器打开，见 `interact`）
//! 与快捷键页的「恢复默认」操作按钮。从 `draw` 拆出，保证单文件不超过 300 行。

use tiny_skia::{Pixmap, Transform};

use crate::editor::Point;
use crate::render::text as dwrite;

use super::Panel;
use super::controls::RowId;

/// 外部链接行：右侧强调色文字 + 「›」箭头，文案按行类型取。
pub(super) fn draw_link(pixmap: &mut Pixmap, panel: &Panel, row: &super::controls::Row) {
    let text = match row.id {
        RowId::Repo => "github.com/bd-dxg/airsnip ›",
        RowId::Licenses => "查看清单 ›",
        _ => "",
    };
    draw_trailing(pixmap, panel, row, text);
}

/// 操作按钮行：左侧标签已由 `draw_row` 用强调色画出，右侧只补一个「›」指示可点。
pub(super) fn draw_action(pixmap: &mut Pixmap, panel: &Panel, row: &super::controls::Row) {
    let text = match row.id {
        RowId::ResetKeys => "›",
        _ => "",
    };
    draw_trailing(pixmap, panel, row, text);
}

/// 右侧对齐的强调色文本。
fn draw_trailing(pixmap: &mut Pixmap, panel: &Panel, row: &super::controls::Row, text: &str) {
    let size = 16.0 * panel.scale;
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
