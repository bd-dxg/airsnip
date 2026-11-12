// SPDX-License-Identifier: GPL-3.0-only
//! 选中对象的编辑装饰（EDT-7 / ADR 0010）：包围框、八向手柄、箭头控制点、圆角手柄。
//!
//! 只上屏显示，**不进导出**（导出由 `draw::bake` 烘焙对象本身）。
//! 位置与命中判定共用 `mod.rs` 的 `handle_points` / `radius_handle_point`。

use airsnip_core::editor::{Kind, Object, Point};
use tiny_skia::{FillRule, Paint, PathBuilder, Stroke, Transform};

use super::{HANDLE, handle_points};

const ACCENT: [u8; 4] = [26, 115, 232, 255];
/// 圆角 / 控制点圆点半径。
const DOT: f32 = 3.5;

/// 画选中对象的编辑框：虚线包围框 + 手柄（箭头/直线只有两端点）。
///
/// 只上屏显示，不进导出（导出由 `draw::bake` 烘焙对象本身）。
pub fn frame(pixmap: &mut tiny_skia::Pixmap, obj: &Object) {
    use tiny_skia::{Paint, Stroke, Transform};
    let accent = [26u8, 115, 232, 255];
    let mut paint = Paint::default();
    paint.set_color_rgba8(accent[0], accent[1], accent[2], accent[3]);

    // 包围框（箭头 / 直线不画框，只给端点）
    if !matches!(obj.kind, Kind::Arrow { .. } | Kind::Line { .. }) {
        let b = obj.bounds();
        let mut pb = tiny_skia::PathBuilder::new();
        pb.move_to(b.x0, b.y0);
        pb.line_to(b.x1, b.y0);
        pb.line_to(b.x1, b.y1);
        pb.line_to(b.x0, b.y1);
        pb.close();
        if let Some(path) = pb.finish() {
            // 先黑后白两遍，深浅底图上都看得见
            let mut dark = Paint::default();
            dark.set_color_rgba8(0, 0, 0, 140);
            pixmap.stroke_path(
                &path,
                &dark,
                &Stroke {
                    width: 2.0,
                    ..Default::default()
                },
                Transform::identity(),
                None,
            );
            pixmap.stroke_path(
                &path,
                &paint,
                &Stroke {
                    width: 1.0,
                    ..Default::default()
                },
                Transform::identity(),
                None,
            );
        }
    }

    for (_, p) in handle_points(obj) {
        let Some(rect) = tiny_skia::Rect::from_xywh(p.x - HANDLE / 2.0, p.y - HANDLE / 2.0, HANDLE, HANDLE) else {
            continue;
        };
        let mut pb = tiny_skia::PathBuilder::new();
        pb.push_rect(rect);
        if let Some(path) = pb.finish() {
            let mut white = Paint::default();
            white.set_color_rgba8(255, 255, 255, 255);
            pixmap.fill_path(&path, &white, tiny_skia::FillRule::Winding, Transform::identity(), None);
            pixmap.stroke_path(
                &path,
                &paint,
                &Stroke {
                    width: 1.0,
                    ..Default::default()
                },
                Transform::identity(),
                None,
            );
        }
    }
}

/// 箭头控制点（EDT-3）：从端点连到控制点的虚线 + 圆点，拖圆点改曲率。
pub fn curve_handles(pixmap: &mut tiny_skia::Pixmap, obj: &Object) {
    let Kind::Arrow { from, to, .. } = &obj.kind else {
        return;
    };
    let Some([c1, c2]) = obj.kind.control_points() else {
        return;
    };
    for (a, b) in [(*from, c1), (c2, *to)] {
        dashed_line(pixmap, a, b);
    }
    for p in [c1, c2] {
        dot(pixmap, p);
    }
}

/// 圆角手柄（ADR 0010）：包围框内侧右上角的小圆。
pub fn radius_handle(pixmap: &mut tiny_skia::Pixmap, obj: &Object) {
    let Some(r) = obj.kind.corner() else {
        return;
    };
    let Some((x, y)) = super::radius_handle_point(obj) else {
        return;
    };
    dot(pixmap, Point::new(x, y));
    if r > 0.5 {
        let b = obj.bounds();
        let mut pb = PathBuilder::new();
        pb.move_to(b.x0 + r, b.y0);
        pb.quad_to(b.x0 + r * 0.45, b.y0 + r * 0.45, b.x0, b.y0 + r);
        if let Some(path) = pb.finish() {
            let mut accent = Paint::default();
            accent.set_color_rgba8(ACCENT[0], ACCENT[1], ACCENT[2], 255);
            pixmap.stroke_path(
                &path,
                &accent,
                &Stroke {
                    width: 1.0,
                    ..Default::default()
                },
                Transform::identity(),
                None,
            );
        }
    }
}

/// 白色圆点 + 蓝边（控制点与圆角手柄共用）。
fn dot(pixmap: &mut tiny_skia::Pixmap, p: Point) {
    let mut pb = PathBuilder::new();
    pb.push_circle(p.x, p.y, DOT);
    if let Some(path) = pb.finish() {
        let mut white = Paint::default();
        white.set_color_rgba8(255, 255, 255, 255);
        pixmap.fill_path(&path, &white, FillRule::Winding, Transform::identity(), None);
        let mut accent = Paint::default();
        accent.set_color_rgba8(ACCENT[0], ACCENT[1], ACCENT[2], 255);
        pixmap.stroke_path(
            &path,
            &accent,
            &Stroke {
                width: 1.0,
                ..Default::default()
            },
            Transform::identity(),
            None,
        );
    }
}

fn dashed_line(pixmap: &mut tiny_skia::Pixmap, a: Point, b: Point) {
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let len = (dx * dx + dy * dy).sqrt();
    if len < 1e-3 {
        return;
    }
    let (ux, uy) = (dx / len, dy / len);
    let mut paint = Paint::default();
    paint.set_color_rgba8(ACCENT[0], ACCENT[1], ACCENT[2], 200);
    let stroke = Stroke {
        width: 1.0,
        ..Default::default()
    };
    let mut t = 0.0;
    while t < len {
        let t2 = (t + 4.0).min(len);
        let mut pb = PathBuilder::new();
        pb.move_to(a.x + ux * t, a.y + uy * t);
        pb.line_to(a.x + ux * t2, a.y + uy * t2);
        if let Some(path) = pb.finish() {
            pixmap.stroke_path(&path, &paint, &stroke, Transform::identity(), None);
        }
        t += 8.0;
    }
}
