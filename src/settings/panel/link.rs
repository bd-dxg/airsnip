// SPDX-License-Identifier: GPL-3.0-only
//! 右侧控件行绘制（M5/SYS-2/SYS-7）：关于页的外部链接入口（点击用浏览器打开，见 `interact`）、
//! 版本更新按钮与快捷键页的「恢复默认」操作按钮。从 `draw` 拆出，保证单文件不超过 300 行。

use tiny_skia::{Pixmap, Transform};

use crate::editor::Point;
use crate::overlay::geometry::SelRect;
use crate::overlay::shape::rounded_rect;
use crate::render::text as dwrite;
use crate::update::state;

use super::Panel;
use super::controls::RowId;

/// 外部链接行：右侧强调色文字 + 「›」箭头，文案按行类型取。
pub(super) fn draw_link(pixmap: &mut Pixmap, panel: &Panel, row: &super::controls::Row) {
    let text = match row.id {
        RowId::Repo => "github.com/bd-dxg/airsnip ›",
        RowId::Licenses => "查看清单 ›",
        _ => "",
    };
    draw_trailing(pixmap, panel, row, text, panel.palette.accent);
}

/// 操作按钮行：左侧标签已由 `draw_row` 用强调色画出，右侧只补一个「›」指示可点。
pub(super) fn draw_action(pixmap: &mut Pixmap, panel: &Panel, row: &super::controls::Row) {
    let text = match row.id {
        RowId::ResetKeys => "›",
        _ => "",
    };
    draw_trailing(pixmap, panel, row, text, panel.palette.accent);
}

/// 版本更新按钮（SYS-7）：真正的按钮外观（圆角底 + 边框 + 居中文字）。
/// 文案随状态变（检查更新 / 更新到 v0.7.0 / 下载中 42% / 重试），忙时置灰。
pub(super) fn draw_button(pixmap: &mut Pixmap, panel: &Panel, row: &super::controls::Row) {
    let p = panel.palette;
    let s = panel.scale;
    let phase = state::phase();
    let (text, enabled) = state::button(&phase);
    let emphasis = state::button_emphasis(&phase);

    // 按钮高度与快捷键行的键帽一致（30px 基准）；宽度按文字量算，右对齐、不超出控件区
    let size = 15.0 * s;
    let spec = dwrite::FontSpec::new(size);
    let (tw, _) = dwrite::measure(&text, &spec);
    let pad = 16.0 * s;
    let c = &row.control;
    let bh = (30.0 * s) as i32;
    let bw = ((tw + pad * 2.0) as i32).clamp((84.0 * s) as i32, c.w);
    let btn = SelRect {
        x: c.x + c.w - bw,
        y: c.y + (c.h - bh) / 2,
        w: bw,
        h: bh,
    };

    let (fill, border, fg) = if !enabled {
        // 忙：素色底 + 灰字，看起来就是「点不了」
        ([p.bg[0], p.bg[1], p.bg[2], 255], p.border, p.text_dim)
    } else if emphasis {
        // 有新版本 / 失败可重试：强调底色
        (p.accent_soft, p.accent, p.accent)
    } else {
        ([p.bg[0], p.bg[1], p.bg[2], 255], p.border, p.text)
    };
    rounded_rect(pixmap, btn, (6.0 * s) as f32, fill, Some(border));

    let x = btn.x as f32 + (bw as f32 - tw) / 2.0;
    let y = btn.y as f32 + (bh as f32 - size * 1.2) / 2.0;
    dwrite::draw(pixmap, Point::new(x, y), &text, &spec, fg, Transform::identity());
}

/// 右侧对齐的指定颜色文本。
fn draw_trailing(pixmap: &mut Pixmap, panel: &Panel, row: &super::controls::Row, text: &str, color: [u8; 4]) {
    let size = 16.0 * panel.scale;
    let (w, _) = dwrite::measure(text, &dwrite::FontSpec::new(size));
    let x = row.control.x + row.control.w as i32 - w as i32;
    let y = row.rect.y as f32 + (row.rect.h as f32 - size * 1.2) / 2.0;
    dwrite::draw(
        pixmap,
        Point::new(x as f32, y),
        text,
        &dwrite::FontSpec::new(size),
        color,
        Transform::identity(),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 各状态的按钮文案在 100% 缩放下都能装进控件区（按钮左缘不会越过标签区）。
    #[test]
    fn button_text_fits_control_width() {
        let phases = [
            state::Phase::Idle,
            state::Phase::Checking,
            state::Phase::UpToDate,
            state::Phase::Available {
                version: "0.7.0".into(),
            },
            state::Phase::Downloading {
                version: "0.7.0".into(),
                percent: 100,
            },
            state::Phase::Installing {
                version: "0.7.0".into(),
            },
            state::Phase::Failed("x".into()),
        ];
        // 控件区宽 260 - 左右 padding 32（与 draw_button 的 pad 一致）
        let limit = 260.0 - 32.0;
        for ph in phases {
            let (text, _) = state::button(&ph);
            let (w, _) = dwrite::measure(&text, &dwrite::FontSpec::new(15.0));
            assert!(w <= limit, "{ph:?} 按钮文案过宽：{w} > {limit}");
        }
    }
}
