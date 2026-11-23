// SPDX-License-Identifier: GPL-3.0-only
//! 徽标绘制（EDT-12 序号标记 / EDT-20 对勾与叉号）：徽标圆 + 引线箭头 + 内容。
//!
//! 从 `render::mod` 拆出，控制单文件规模。屏幕显示、导出烘焙、工具栏下拉预览共用一份绘制，
//! 三处观感不会分叉。
//!
//! **对勾 / 叉号用矢量描边，不用字符**：字体族缺字时会变成豆腐块，且不同字体下 ✓ / ✕ 的
//! 宽度与基线差很多，徽标里的视觉重量和数字对不上（ADR 0045）。

use tiny_skia::{FillRule, LineCap, LineJoin, PathBuilder, Pixmap, Stroke, Transform};

use crate::editor::{Badge, Kind, Object, Point, Style};

use super::{box_span, draw_arrow, paint, paint_of, text};

/// 对勾折线：相对徽标半径的比例坐标（左下起 → 底拐点 → 右上收尾）。
const MARK_CHECK: [(f32, f32); 3] = [(-0.42, 0.02), (-0.12, 0.34), (0.44, -0.34)];
/// 叉号：两条独立线段（`move_to` 断开），比例坐标相对徽标半径。
const MARK_CROSS: [[(f32, f32); 2]; 2] = [[(-0.34, -0.34), (0.34, 0.34)], [(-0.34, 0.34), (0.34, -0.34)]];
/// 符号描边粗细 / 徽标半径（约等于数字的笔画重量）。
const MARK_STROKE_K: f32 = 0.30;

/// 徽标圆半径：按内容宽度与字号测量后取值（缓存进对象，供几何 / 命中复用）。
/// 对勾 / 叉号按「与单个数字等宽」估（`0.6 × 字号`），与数字的视觉重量对齐。
pub fn number_radius(font_size: f32, mark: Badge) -> f32 {
    let w = match mark {
        Badge::Num(n) => {
            text::measure(&n.to_string(), &{
                let mut spec = text::FontSpec::content(font_size);
                spec.bold = true;
                spec
            })
            .0
        }
        Badge::Check | Badge::Cross => font_size * 0.6,
    };
    (w * 0.5 + font_size * 0.35).max(font_size * 0.8)
}

/// 画一个徽标对象：先画徽标边缘引出的箭头（复用直线箭头绘制），再画实心圆与内容。
pub(super) fn draw_number(pixmap: &mut Pixmap, obj: &Object, tr: Transform) {
    let Kind::Number {
        center,
        tip,
        mark,
        font_size,
        radius,
    } = &obj.kind
    else {
        return;
    };
    let (center, tip) = (*center, *tip);
    // 1) 引线箭头：起点在徽标边缘，箭头头部随线宽（直线控制点置于 1/3、2/3 处）
    let start = crate::editor::number_arrow_start(center, tip, *radius);
    let (dx, dy) = (tip.x - start.x, tip.y - start.y);
    if dx * dx + dy * dy >= 1.0 {
        let c1 = Point::new(start.x + dx / 3.0, start.y + dy / 3.0);
        let c2 = Point::new(start.x + dx * 2.0 / 3.0, start.y + dy * 2.0 / 3.0);
        draw_arrow(pixmap, start, c1, c2, tip, &obj.style, tr);
    }
    // 2) 徽标圆 + 内容
    let r = *radius;
    let (ga, gb) = box_span(center.x - r, center.y - r, center.x + r, center.y + r);
    let paint = paint_of(&obj.style, ga, gb);
    draw_badge_face(pixmap, center, r, *mark, *font_size, &paint, &obj.style, tr);
}

/// 徽标圆 + 内容：内容颜色按徽标底色亮度自动取反色（白底用黑字，否则白字），
/// 避免白徽标上看不见。
fn draw_badge_face(
    pixmap: &mut Pixmap,
    center: Point,
    radius: f32,
    mark: Badge,
    font_size: f32,
    paint: &tiny_skia::Paint,
    style: &Style,
    tr: Transform,
) {
    let mut pb = PathBuilder::new();
    pb.push_circle(center.x, center.y, radius);
    if let Some(path) = pb.finish() {
        pixmap.fill_path(&path, paint, FillRule::Winding, tr, None);
    }
    let ink = contrast_ink(style.color);
    match mark {
        Badge::Num(n) => {
            let mut spec = text::FontSpec::content(font_size);
            spec.bold = true;
            let label = n.to_string();
            let (tw, th) = text::measure(&label, &spec);
            let pos = Point::new(center.x - tw * 0.5, center.y - th * 0.5 + font_size * 0.08);
            text::draw(pixmap, pos, &label, &spec, ink, tr);
        }
        Badge::Check => draw_mark(pixmap, center, radius, &[&MARK_CHECK], ink, tr),
        Badge::Cross => draw_mark(pixmap, center, radius, &[&MARK_CROSS[0], &MARK_CROSS[1]], ink, tr),
    }
}

/// 工具栏徽标预览（EDT-20）：格子里的当前形态与下拉项的缩略图。
/// `fill` 为徽标底色、`ink` 为内容色（调用方已按悬停 / 选中态挑好）。
pub(crate) fn badge_preview(pixmap: &mut Pixmap, center: Point, radius: f32, mark: Badge, fill: [u8; 4], ink: [u8; 4]) {
    let paint = paint::solid_paint(fill);
    let mut pb = PathBuilder::new();
    pb.push_circle(center.x, center.y, radius);
    if let Some(path) = pb.finish() {
        pixmap.fill_path(&path, &paint, FillRule::Winding, Transform::identity(), None);
    }
    let tr = Transform::identity();
    match mark {
        Badge::Num(_) => {
            // 预览里的编号固定画 1：它只是「这一格是数字」的示意，不代表下一个号的真实值
            let mut spec = text::FontSpec::content(radius * 1.5);
            spec.bold = true;
            let (tw, th) = text::measure("1", &spec);
            let pos = Point::new(center.x - tw * 0.5, center.y - th * 0.5 + radius * 1.5 * 0.08);
            text::draw(pixmap, pos, "1", &spec, ink, tr);
        }
        Badge::Check => draw_mark(pixmap, center, radius, &[&MARK_CHECK], ink, tr),
        Badge::Cross => draw_mark(pixmap, center, radius, &[&MARK_CROSS[0], &MARK_CROSS[1]], ink, tr),
    }
}

/// 画徽标符号：多段折线共用一条路径，圆头端点 + 圆角连接，不依赖字体。
fn draw_mark(pixmap: &mut Pixmap, center: Point, r: f32, lines: &[&[(f32, f32)]], ink: [u8; 4], tr: Transform) {
    let mut pb = PathBuilder::new();
    for line in lines {
        for (i, (dx, dy)) in line.iter().enumerate() {
            let (x, y) = (center.x + dx * r, center.y + dy * r);
            if i == 0 {
                pb.move_to(x, y);
            } else {
                pb.line_to(x, y);
            }
        }
    }
    let Some(path) = pb.finish() else { return };
    let stroke = Stroke {
        width: r * MARK_STROKE_K,
        line_cap: LineCap::Round,
        line_join: LineJoin::Round,
        ..Default::default()
    };
    pixmap.stroke_path(&path, &paint::solid_paint(ink), &stroke, tr, None);
}

/// 徽标底色对应的内容颜色：按感知亮度（BT.601）在白/黑间取反色。
/// 工具栏格子与下拉的徽标缩略图也用它（`badge_preview` 的调用方）。
pub(crate) fn contrast_ink(badge: [u8; 4]) -> [u8; 4] {
    let lum = (badge[0] as u32 * 299 + badge[1] as u32 * 587 + badge[2] as u32 * 114) / 1000;
    if lum >= 150 {
        [0, 0, 0, 255]
    } else {
        [255, 255, 255, 255]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn new_pixmap(w: u32, h: u32) -> Pixmap {
        let mut p = Pixmap::new(w, h).unwrap();
        p.fill(tiny_skia::Color::TRANSPARENT);
        p
    }

    fn pixel(p: &Pixmap, x: u32, y: u32) -> (u8, u8, u8, u8) {
        let off = ((y * p.width() + x) * 4) as usize;
        (p.data()[off], p.data()[off + 1], p.data()[off + 2], p.data()[off + 3])
    }

    fn badge_obj(center: Point, tip: Point, mark: Badge, color: [u8; 4]) -> Object {
        Object {
            kind: Kind::Number {
                center,
                tip,
                mark,
                font_size: 20.0,
                radius: number_radius(20.0, mark),
            },
            style: Style::new(color, 3.0),
        }
    }

    /// 徽标圆半径随编号位数增大（一位 → 两位 → 三位），符号与一位数字同半径。
    #[test]
    fn number_radius_grows_with_digits() {
        let one = number_radius(20.0, Badge::Num(1));
        let two = number_radius(20.0, Badge::Num(10));
        let three = number_radius(20.0, Badge::Num(100));
        assert!(one > 0.0, "半径应非零：{one}");
        assert!(two >= one && three >= two, "位数越多半径不应更小：{one}/{two}/{three}");
        assert_eq!(number_radius(20.0, Badge::Check), one, "对勾应与一位数字同半径");
        assert_eq!(number_radius(20.0, Badge::Cross), one, "叉号应与一位数字同半径");
    }

    /// 徽标为实心圆、引线画到尖端，区域外不被污染。
    #[test]
    fn number_draw_paints_badge_and_leader() {
        let mut p = new_pixmap(120, 120);
        p.fill(tiny_skia::Color::from_rgba8(0, 0, 0, 255));
        let obj = badge_obj(
            Point::new(40.0, 60.0),
            Point::new(112.0, 60.0),
            Badge::Num(1),
            [200, 30, 30, 255],
        );
        crate::render::draw_object(&mut p, &obj);
        // 徽标圆心附近应为红色（实心圆）
        let (r, g, b, _) = pixel(&p, 28, 60);
        assert!(r > 150 && g < 100 && b < 100, "徽标应为红色：{r},{g},{b}");
        // 引线尖端附近应有像素
        assert!(pixel(&p, 108, 60).0 > 0, "引线箭头处应有像素");
        // 远离对象处保持黑色
        assert_eq!(pixel(&p, 5, 5), (0, 0, 0, 255));
    }

    /// 数字颜色随徽标底色自动反色（白底绘深色数字）。
    #[test]
    fn number_digit_contrasts_with_badge() {
        let mut p = new_pixmap(60, 60);
        let obj = badge_obj(
            Point::new(30.0, 30.0),
            Point::new(30.0, 30.0),
            Badge::Num(1),
            [255, 255, 255, 255],
        );
        crate::render::draw_object(&mut p, &obj);
        let has_dark = (0..60).any(|y| (0..60).any(|x| pixel(&p, x, y).0 < 80 && pixel(&p, x, y).3 > 0));
        assert!(has_dark, "白底徽标上应绘制深色数字");
    }

    /// 对勾 / 叉号走矢量描边：红徽标上应是反白像素、圆外不污染。
    #[test]
    fn badge_symbols_draw_contrast_ink() {
        for mark in [Badge::Check, Badge::Cross] {
            let mut p = new_pixmap(60, 60);
            p.fill(tiny_skia::Color::from_rgba8(0, 0, 0, 255));
            let obj = badge_obj(Point::new(30.0, 30.0), Point::new(30.0, 30.0), mark, [200, 30, 30, 255]);
            crate::render::draw_object(&mut p, &obj);
            let has_ink = (0..60).any(|y| {
                (0..60).any(|x| {
                    let (r, g, b, a) = pixel(&p, x, y);
                    a > 0 && r > 200 && g > 200 && b > 200
                })
            });
            assert!(has_ink, "{mark:?} 应在徽标上画出反白笔画");
            assert_eq!(pixel(&p, 1, 1), (0, 0, 0, 255), "圆外不应被污染");
        }
    }
}
