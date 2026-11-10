// SPDX-License-Identifier: GPL-3.0-only
//! 命中测试（EDT-3a/EDT-7）：把对象几何转换为「点到图形距离 ≤ 容差」的判定。
//! 曲线箭头复用 `curve` 的展平折线，与渲染使用同一参数，保证所见即所点。

use super::{Kind, Point, curve};

impl Kind {
    /// 点 `q` 是否命中对象（`tol` 为额外视觉容差，不含线宽）。
    pub fn hit(&self, q: Point, tol: f32) -> bool {
        match self {
            Kind::Rect { a, b, filled, .. } => hit_rect(*a, *b, *filled, q, tol),
            Kind::Arrow { from, c1, c2, to } => {
                curve::dist_to_polyline(&curve::flatten(*from, *c1, *c2, *to), q) <= tol
            }
            // 区域类（高亮/模糊/聚光）：整块区域都可命中（同填充矩形），便于选中拖动
            Kind::Highlight { a, b, .. } => hit_rect(*a, *b, true, q, tol),
            Kind::Blur { a, b, .. } => hit_rect(*a, *b, true, q, tol),
            Kind::Glow { a, b, .. } => hit_rect(*a, *b, true, q, tol),
            // 文本：整个文字矩形都可命中（同填充矩形），尺寸来自测量缓存
            Kind::Text { pos, size, .. } => {
                let (a, b) = super::text::rect(*pos, *size);
                hit_rect(a, b, true, q, tol)
            }
            // 序号标记（EDT-12）：徽标圆内或引线线段附近都算命中
            Kind::Number {
                center, tip, radius, ..
            } => {
                let (dx, dy) = (q.x - center.x, q.y - center.y);
                let r = *radius + tol;
                dx * dx + dy * dy <= r * r
                    || curve::dist_to_polyline(&[super::number_arrow_start(*center, *tip, *radius), *tip], q) <= tol
            }
            // EDT-13 直线：点到线段距离
            Kind::Line { from, to } => curve::dist_to_polyline(&[*from, *to], q) <= tol,
            // EDT-14 椭圆：外接矩形填充时内部算命中，否则只判描边（采样折线）
            Kind::Ellipse { a, b, filled, .. } => hit_ellipse(*a, *b, *filled, q, tol),
            // EDT-15 手绘：点到折线距离
            Kind::Freehand { pts } => curve::dist_to_polyline(pts.as_slice(), q) <= tol,
        }
    }
}

/// 椭圆采样段数（命中测试用）：整圈 64 段，屏幕尺度下弦高远小于命中容差
/// （半径 2000px 时约 2.4px，额内容差 ≥ 4px）。
const ELLIPSE_SEGMENTS: usize = 64;

/// 椭圆到点的距离：按参数方程逐段采样折线再判距；填充时内部算命中。
/// 逐段判定而不先物化整个点列：本函数在鼠标移动的命中路径上对每个对象调用一次，
/// 每帧 N 次堆分配没有必要。
fn hit_ellipse(a: Point, b: Point, filled: bool, q: Point, tol: f32) -> bool {
    let (cx, cy) = ((a.x + b.x) * 0.5, (a.y + b.y) * 0.5);
    let (rx, ry) = ((b.x - a.x).abs() * 0.5, (b.y - a.y).abs() * 0.5);
    if rx < 1e-6 || ry < 1e-6 {
        return false;
    }
    // 填充判定：归一化椭圆不等式 (x/rx)² + (y/ry)² ≤ 1
    if filled {
        let (dx, dy) = ((q.x - cx) / rx, (q.y - cy) / ry);
        if dx * dx + dy * dy <= 1.0 {
            return true;
        }
    }
    // 包围盒早退：屏幕上的椭圆大多时候离鼠标很远，先排除矩形区域外的点
    if q.x < cx - rx - tol || q.x > cx + rx + tol || q.y < cy - ry - tol || q.y > cy + ry + tol {
        return false;
    }
    let step = std::f32::consts::TAU / ELLIPSE_SEGMENTS as f32;
    let (mut prev_x, mut prev_y) = (cx + rx, cy);
    for i in 1..=ELLIPSE_SEGMENTS {
        let t = i as f32 * step;
        let (next_x, next_y) = (cx + rx * t.cos(), cy + ry * t.sin());
        let seg = curve::dist_to_segment(q, Point::new(prev_x, prev_y), Point::new(next_x, next_y));
        if seg <= tol {
            return true;
        }
        prev_x = next_x;
        prev_y = next_y;
    }
    false
}

/// 矩形：填充时内部也算命中，否则只判描边。
fn hit_rect(a: Point, b: Point, filled: bool, q: Point, tol: f32) -> bool {
    let (x0, x1) = (a.x.min(b.x), a.x.max(b.x));
    let (y0, y1) = (a.y.min(b.y), a.y.max(b.y));
    if filled && q.x >= x0 && q.x <= x1 && q.y >= y0 && q.y <= y1 {
        return true;
    }
    let corners = [
        Point::new(x0, y0),
        Point::new(x1, y0),
        Point::new(x1, y1),
        Point::new(x0, y1),
        Point::new(x0, y0),
    ];
    curve::dist_to_polyline(&corners, q) <= tol
}
