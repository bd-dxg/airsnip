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

/// 按钮行（`RowKind::Button`）的按钮文案与状态：「恢复默认」为固定文案，其余随更新状态变。
fn button_state(row: &super::controls::Row) -> (String, bool, bool) {
    if row.id == RowId::ResetKeys {
        return ("恢复默认".to_string(), true, false);
    }
    let phase = state::phase();
    let (text, enabled) = state::button(&phase);
    (text, enabled, state::button_emphasis(&phase))
}

/// 按钮几何（纯计算，便于测试）：高度与快捷键行的键帽一致（30px 基准），宽度按文字量算，
/// 右对齐、不超出控件区。
fn btn_geom(control: SelRect, text_w: f32, s: f32) -> SelRect {
    let pad = 16.0 * s;
    let bh = (30.0 * s) as i32;
    let bw = ((text_w + pad * 2.0) as i32).clamp((90.0 * s) as i32, control.w);
    SelRect {
        x: control.x + control.w - bw,
        y: control.y + (control.h - bh) / 2,
        w: bw,
        h: bh,
    }
}

/// 按钮行的按钮矩形：绘制与鼠标命中共用这一份几何，保证「看得见的边界 == 点得中的边界」。
/// 命中测试必须用它——曾经整行可点，按钮外一点就触发（比按钮自身宽得多）。
pub(super) fn button_rect(panel: &Panel, row: &super::controls::Row) -> SelRect {
    button_geom(panel, row).0
}

/// 按钮矩形 + 文案宽度（绘制需要宽度来居中文字，不重复测量）。
fn button_geom(panel: &Panel, row: &super::controls::Row) -> (SelRect, f32) {
    let s = panel.scale;
    let (text, ..) = button_state(row);
    let (tw, _) = dwrite::measure(&text, &dwrite::FontSpec::new(15.0 * s));
    (btn_geom(row.control, tw, s), tw)
}

/// 按钮风格行：真正的按钮外观（圆角底 + 边框 + 居中文字）。
/// 「恢复默认」（快捷键页）为固定文案；「版本更新」（关于页）文案随状态变、忙时置灰。
/// `hovered`：鼠标悬停在按钮上时给强调底色，明确「可点」（悬停在行内空白处不算）。
pub(super) fn draw_button(pixmap: &mut Pixmap, panel: &Panel, row: &super::controls::Row, hovered: bool) {
    let p = panel.palette;
    let s = panel.scale;
    let (text, enabled, emphasis) = button_state(row);
    let (btn, tw) = button_geom(panel, row);
    let size = 15.0 * s;
    let spec = dwrite::FontSpec::new(size);

    let (fill, border, fg) = if !enabled {
        // 忙：素色底 + 灰字，看起来就是「点不了」
        ([p.bg[0], p.bg[1], p.bg[2], 255], p.border, p.text_dim)
    } else if emphasis || hovered {
        // 悬停，或「有新版本 / 失败可重试」：强调底色
        (p.accent_soft, p.accent, p.accent)
    } else {
        ([p.bg[0], p.bg[1], p.bg[2], 255], p.border, p.text)
    };
    rounded_rect(pixmap, btn, (6.0 * s) as f32, fill, Some(border));

    let x = btn.x as f32 + (btn.w as f32 - tw) / 2.0;
    let y = btn.y as f32 + (btn.h as f32 - size * 1.2) / 2.0;
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

    /// 按钮矩形不占满行（否则会出现「按钮外一点就触发」）：右对齐、比行矮、比控件区窄。
    #[test]
    fn button_rect_stays_inside_control() {
        // 标准行的控件区：260 宽 × 46 高，起点在行右侧
        let c = SelRect {
            x: 400,
            y: 0,
            w: 260,
            h: 46,
        };
        let r = btn_geom(c, 60.0, 1.0);
        assert_eq!(r.x + r.w, c.x + c.w, "按钮应与控件区右对齐");
        assert!(r.x > c.x, "按钮左缘不应贴到控件区左缘：{r:?}");
        assert!(r.h < c.h, "按钮应比行矮：{r:?}");
        // 文案极长时钳到控件区宽度（仍不越出控件区、更不占满整行）
        let wide = btn_geom(c, 999.0, 1.0);
        assert_eq!(wide.w, c.w);
        assert_eq!(wide.x + wide.w, c.x + c.w);
        // 高 DPI 下同样成立（控件区几何也跟着缩放）
        let hi_c = SelRect {
            x: 800,
            y: 0,
            w: 520,
            h: 92,
        };
        let hi = btn_geom(hi_c, 120.0, 2.0);
        assert_eq!(hi.x + hi.w, hi_c.x + hi_c.w);
        assert!(hi.x > hi_c.x && hi.h < hi_c.h);
    }

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
