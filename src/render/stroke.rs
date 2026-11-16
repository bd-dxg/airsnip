// SPDX-License-Identifier: GPL-3.0-only
//! 描边类基础图形渲染（EDT-13/14/15）：直线、椭圆、手绘自由笔画。
//! 从 `render::mod` 拆出，控制单文件规模；共同点是都用 `Stroke` 描边（椭圆可填充）。

use tiny_skia::{FillRule, LineCap, LineJoin, PathBuilder, Pixmap, Rect, Transform};

use super::{box_span, paint_of, stroke_of};
use crate::editor::{Point, Style};

/// EDT-13 直线：无箭头的纯线段，端点用圆端帽（避免描边超出端点的生硬截断）。
pub(super) fn draw_line(pixmap: &mut Pixmap, from: Point, to: Point, style: &Style, tr: Transform) {
    let (dx, dy) = (to.x - from.x, to.y - from.y);
    if dx * dx + dy * dy < 0.25 {
        return;
    }
    let mut pb = PathBuilder::new();
    pb.move_to(from.x, from.y);
    pb.line_to(to.x, to.y);
    let Some(path) = pb.finish() else {
        return;
    };
    let paint = paint_of(style, from, to);
    let stroke = stroke_of(style, LineCap::Round, LineJoin::Round);
    pixmap.stroke_path(&path, &paint, &stroke, tr, None);
}

/// 折线包围盒：渐变方向用它（折返的手绘不让渐变跟着倒退）。
fn points_bounds(pts: &[Point]) -> (Point, Point) {
    let first = pts[0];
    let (mut x0, mut y0, mut x1, mut y1) = (first.x, first.y, first.x, first.y);
    for p in pts {
        (x0, y0) = (x0.min(p.x), y0.min(p.y));
        (x1, y1) = (x1.max(p.x), y1.max(p.y));
    }
    (Point::new(x0, y0), Point::new(x1, y1))
}

/// EDT-14 椭圆：外接矩形对角两点，`filled` 时填充（半透明由颜色 alpha 表达，与矩形一致）。
pub(super) fn draw_ellipse(pixmap: &mut Pixmap, a: Point, b: Point, filled: bool, style: &Style, tr: Transform) {
    let (x0, y0) = (a.x.min(b.x), a.y.min(b.y));
    let (x1, y1) = (a.x.max(b.x), a.y.max(b.y));
    if x1 - x0 < 1.0 || y1 - y0 < 1.0 {
        return;
    }
    let Some(rect) = Rect::from_ltrb(x0, y0, x1, y1) else {
        return;
    };
    let mut pb = PathBuilder::new();
    pb.push_oval(rect);
    let Some(path) = pb.finish() else {
        return;
    };
    let (ga, gb) = box_span(x0, y0, x1, y1);
    let paint = paint_of(style, ga, gb);
    if filled {
        pixmap.fill_path(&path, &paint, FillRule::Winding, tr, None);
        return;
    }
    // 渐变描边沿周长弧长走色（否则水平渐变下上下两边同色，ADR 0043）
    if style.is_gradient() {
        super::gradient_stroke::stroke_gradient(pixmap, &path, style, tr);
        return;
    }
    let stroke = stroke_of(style, LineCap::Butt, LineJoin::Miter);
    pixmap.stroke_path(&path, &paint, &stroke, tr, None);
}

/// EDT-15 手绘：用「相邻点中点 + 二次贝塞尔」平滑折线。
/// 从首点出发，每对相邻点的中点用二次贝塞尔连接（控制点取原始点），
/// 只在相邻点有实际距离时绘制，避免重合点产生退化线段。
pub(super) fn draw_freehand(pixmap: &mut Pixmap, pts: &[Point], style: &Style, tr: Transform) {
    if pts.len() < 2 {
        return;
    }
    let mut pb = PathBuilder::new();
    pb.move_to(pts[0].x, pts[0].y);
    let mut emitted = false;
    for i in 1..pts.len() {
        let (p0, p1) = (pts[i - 1], pts[i]);
        let (dx, dy) = (p1.x - p0.x, p1.y - p0.y);
        if dx * dx + dy * dy < 0.25 {
            continue;
        }
        let (mx, my) = ((p0.x + p1.x) * 0.5, (p0.y + p1.y) * 0.5);
        if !emitted {
            // 第一段：从中点开始直线到首个中点（保留首点端点）
            pb.line_to(mx, my);
            emitted = true;
        } else {
            pb.quad_to(p0.x, p0.y, mx, my);
        }
    }
    if !emitted {
        return;
    }
    let last = pts[pts.len() - 1];
    pb.line_to(last.x, last.y);
    let Some(path) = pb.finish() else {
        return;
    };
    let (ga, gb) = points_bounds(pts);
    let (ga, gb) = box_span(ga.x, ga.y, gb.x, gb.y);
    let paint = paint_of(style, ga, gb);
    // 手绘是开路径：沿笔迹弧长走色（与矩形 / 椭圆一致）
    if style.is_gradient() {
        super::gradient_stroke::stroke_gradient(pixmap, &path, style, tr);
        return;
    }
    let stroke = stroke_of(style, LineCap::Round, LineJoin::Round);
    pixmap.stroke_path(&path, &paint, &stroke, tr, None);
}
