// SPDX-License-Identifier: GPL-3.0-only
//! 描边类基础图形渲染（EDT-13/14/15）：直线、椭圆、手绘自由笔画。
//! 从 `render::mod` 拆出，控制单文件规模；共同点是都用 `Stroke` 描边（椭圆可填充）。

use tiny_skia::{FillRule, LineCap, LineJoin, PathBuilder, Pixmap, Rect, Stroke, Transform};

use super::paint_of;
use crate::editor::{Point, Style};

/// EDT-13 直线：无箭头的纯线段，端点用圆端帽（避免描边超出端点的生硬截断）。
pub fn draw_line(pixmap: &mut Pixmap, from: Point, to: Point, style: &Style, tr: Transform) {
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
    let paint = paint_of(style.color);
    let stroke = Stroke {
        width: style.width,
        line_cap: LineCap::Round,
        ..Default::default()
    };
    pixmap.stroke_path(&path, &paint, &stroke, tr, None);
}

/// EDT-14 椭圆：外接矩形对角两点，`filled` 时填充（半透明由颜色 alpha 表达，与矩形一致）。
pub fn draw_ellipse(pixmap: &mut Pixmap, a: Point, b: Point, filled: bool, style: &Style, tr: Transform) {
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
    let paint = paint_of(style.color);
    if filled {
        pixmap.fill_path(&path, &paint, FillRule::Winding, tr, None);
        return;
    }
    let stroke = Stroke {
        width: style.width,
        ..Default::default()
    };
    pixmap.stroke_path(&path, &paint, &stroke, tr, None);
}

/// EDT-15 手绘：用「相邻点中点 + 二次贝塞尔」平滑折线。
/// 从首点出发，每对相邻点的中点用二次贝塞尔连接（控制点取原始点），
/// 只在相邻点有实际距离时绘制，避免重合点产生退化线段。
pub fn draw_freehand(pixmap: &mut Pixmap, pts: &[Point], style: &Style, tr: Transform) {
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
    let paint = paint_of(style.color);
    let stroke = Stroke {
        width: style.width,
        line_cap: LineCap::Round,
        line_join: LineJoin::Round,
        ..Default::default()
    };
    pixmap.stroke_path(&path, &paint, &stroke, tr, None);
}
