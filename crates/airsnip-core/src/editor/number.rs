// SPDX-License-Identifier: GPL-3.0-only
//! 序号标记几何（EDT-12）：徽标圆 + 引线箭头的包围盒、引线起点与编号分配。
//! 从 `editor::mod` 拆出，保证单文件不超过 300 行。

use super::{Bounds, Kind, Object, Point, Style, arrow_head_len};

/// 徽标圆与引线箭头的并集包围盒（引线起点在徽标边缘，箭头头部外扩随线宽）。
pub(super) fn bounds(center: Point, tip: Point, radius: f32, style: &Style) -> Bounds {
    let half = style.width * 0.5 + 1.0;
    let badge = Bounds::around(
        center.x - radius,
        center.y - radius,
        center.x + radius,
        center.y + radius,
        half,
    );
    let start = number_arrow_start(center, tip, radius);
    let (dx, dy) = (tip.x - start.x, tip.y - start.y);
    if dx * dx + dy * dy < 1.0 {
        return badge; // 只有徽标（未拖动引线）
    }
    let pad = half.max(arrow_head_len(style.width) * 0.5);
    let arrow = Bounds::around(
        start.x.min(tip.x),
        start.y.min(tip.y),
        start.x.max(tip.x),
        start.y.max(tip.y),
        pad,
    );
    Bounds {
        x0: badge.x0.min(arrow.x0),
        y0: badge.y0.min(arrow.y0),
        x1: badge.x1.max(arrow.x1),
        y1: badge.y1.max(arrow.y1),
    }
}

/// 引线起点（EDT-12）：徽标圆边缘上朝 `tip` 方向的点，避免引线盖住数字；
/// `tip` 与圆心重合（只点一下没拖）时返回圆心。
pub fn number_arrow_start(center: Point, tip: Point, radius: f32) -> Point {
    let (dx, dy) = (tip.x - center.x, tip.y - center.y);
    let len = (dx * dx + dy * dy).sqrt();
    if len < 1e-3 {
        return center;
    }
    Point::new(center.x + dx / len * radius, center.y + dy / len * radius)
}

/// 下一个序号（EDT-12）：取已有对象里序号的最大值 +1，删除 / 撤销中间的序号不重排。
pub fn next_number(objects: &[Object]) -> u32 {
    objects
        .iter()
        .filter_map(|o| match &o.kind {
            Kind::Number { n, .. } => Some(*n),
            _ => None,
        })
        .max()
        .unwrap_or(0)
        + 1
}
