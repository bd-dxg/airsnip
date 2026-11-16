// SPDX-License-Identifier: GPL-3.0-only
//! 线型样本（EDT-18）：工具栏格子与线型下拉项共用同一段绘制。
//! 与真实标注走同一套 `render::stroke_of`（含 dash），所以样本不会与出图观感分叉。

use tiny_skia::{LineCap, LineJoin, PathBuilder, Pixmap, Transform};

use crate::editor::{LineStyle, Style};
use crate::overlay::SelRect;
use crate::overlay::ui::Ui;
use crate::render::{paint_of, stroke_of};

/// 在矩形内画一段水平线型样本（左右留边、垂直居中）。
/// 线宽固定约 2 个基准像素：样本是「符号」，不跟真实线宽走，否则 20px 粗线在 26px 格子里看不出虚实。
pub(super) fn draw_sample(pixmap: &mut Pixmap, r: SelRect, line: LineStyle, color: [u8; 4], ui: Ui) {
    let margin = ui.px(5.0);
    let (x0, x1) = ((r.x + margin) as f32, (r.x + r.w - margin) as f32);
    if x1 - x0 < 2.0 {
        return;
    }
    let y = r.y as f32 + r.h as f32 / 2.0;
    let mut pb = PathBuilder::new();
    pb.move_to(x0, y);
    pb.line_to(x1, y);
    let Some(path) = pb.finish() else {
        return;
    };
    let style = Style::new(color, ui.f(2.0)).with_line(line);
    let stroke = stroke_of(&style, LineCap::Butt, LineJoin::Miter);
    pixmap.stroke_path(&path, &paint_of(color), &stroke, Transform::identity(), None);
}

/// 格子右侧的下拉指示箭头（▾）：告诉用户这一格点得开。
/// 线路样本不占这块：工具栏分支传入细过的矩形（见 `toolbar::draw`）。
pub(super) fn draw_chevron(pixmap: &mut Pixmap, r: SelRect, color: [u8; 4], ui: Ui) {
    let cx = r.x as f32 + r.w as f32 - ui.f(6.5);
    let cy = r.y as f32 + r.h as f32 / 2.0;
    let (dx, dy) = (ui.f(2.6), ui.f(1.5));
    let mut pb = PathBuilder::new();
    pb.move_to(cx - dx, cy - dy);
    pb.line_to(cx, cy + dy);
    pb.line_to(cx + dx, cy - dy);
    let Some(path) = pb.finish() else {
        return;
    };
    let stroke = tiny_skia::Stroke {
        width: ui.f(1.4).max(1.0),
        line_cap: LineCap::Round,
        line_join: LineJoin::Round,
        ..Default::default()
    };
    pixmap.stroke_path(&path, &paint_of(color), &stroke, Transform::identity(), None);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base() -> Ui {
        Ui::from_dpi(96)
    }

    /// 箭头区与样本不重叠：样本线不越过收缩后的右界，箭头区必须有墨。
    #[test]
    fn chevron_sits_right_of_sample() {
        let ui = base();
        let r = SelRect {
            x: 10,
            y: 10,
            w: 52,
            h: 26,
        };
        let sample = SelRect {
            w: r.w - ui.px(9.0),
            ..r
        };
        let mut pm = Pixmap::new(80, 50).unwrap();
        pm.fill(tiny_skia::Color::TRANSPARENT);
        draw_sample(&mut pm, sample, LineStyle::Solid, [255, 0, 0, 255], ui);

        // 样本所在中线：收缩后的右界之外应为空（箭头不画在中线上）
        let y = (r.y + r.h / 2) as u32;
        for x in (sample.x + sample.w) as u32..(r.x + r.w) as u32 {
            assert!(
                pm.pixel(x, y).is_none_or(|p| p.alpha() == 0),
                "样本不应画到箭头区：x={x}"
            );
        }

        // 箭头自身有墨（否则「可点开」的提示就没了）
        draw_chevron(&mut pm, r, [255, 0, 0, 255], ui);
        let mut ink = 0;
        for x in (sample.x + sample.w) as u32..(r.x + r.w) as u32 {
            for y in r.y as u32..(r.y + r.h) as u32 {
                if pm.pixel(x, y).is_some_and(|p| p.alpha() > 0) {
                    ink += 1;
                }
            }
        }
        assert!(ink > 0, "下拉箭头应该画出来");
    }
}
