// SPDX-License-Identifier: GPL-3.0-only
//! 选中对象的控制柄追加绘制：曲线控制点圆点 + 虚线辅助线（EDT-3）。
//! 从 `pick` 拆出，保证单文件不超过 300 行；命中判定仍留在父模块。
//! 只在屏幕显示层绘制，不进导出烘焙。

use tiny_skia::Pixmap;
use tiny_skia::{Paint, PathBuilder, Stroke, Transform};

use crate::editor::{Kind, Object, Point};

use super::super::ui::Ui;
use super::{ACCENT, ACCENT_B, ACCENT_G, CTRL_RADIUS_BASE};

/// 选中箭头的控制柄（EDT-3）：控制点处画圆点，与端点之间用虚线辅助线连接。
pub(in crate::overlay) fn draw_curve_handles(pixmap: &mut Pixmap, obj: &Object, ui: Ui) {
    let (Some(cps), Kind::Arrow { from, to, .. }) = (obj.kind.control_points(), &obj.kind) else {
        return;
    };
    let [c1, c2] = cps;
    dashed_line(pixmap, *from, c1, ui);
    dashed_line(pixmap, c2, *to, ui);
    for p in [c1, c2] {
        let mut pb = PathBuilder::new();
        pb.push_circle(p.x, p.y, ui.f(CTRL_RADIUS_BASE));
        let Some(path) = pb.finish() else {
            continue;
        };
        let mut fill = Paint::default();
        fill.set_color_rgba8(255, 255, 255, 255);
        pixmap.fill_path(&path, &fill, tiny_skia::FillRule::Winding, Transform::identity(), None);
        let mut edge = Paint::default();
        edge.set_color_rgba8(ACCENT, ACCENT_G, ACCENT_B, 255);
        let stroke = Stroke {
            width: ui.f(1.5),
            ..Default::default()
        };
        pixmap.stroke_path(&path, &edge, &stroke, Transform::identity(), None);
    }
}

/// 虚线辅助线（5px 实 / 4px 虚）：tiny-skia 无 dash，手动拆成小段。
pub(in crate::overlay) fn dashed_line(pixmap: &mut Pixmap, a: Point, b: Point, ui: Ui) {
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let len = (dx * dx + dy * dy).sqrt();
    if len < 1.0 {
        return;
    }
    let (ux, uy) = (dx / len, dy / len);
    // 实线段与周期按界面缩放，避免 4K 下虚线看起来是实线
    let (dash, period) = (ui.f(5.0), ui.f(9.0));
    let mut pb = PathBuilder::new();
    let mut t = 0.0;
    while t < len {
        let end = (t + dash).min(len);
        pb.move_to(a.x + ux * t, a.y + uy * t);
        pb.line_to(a.x + ux * end, a.y + uy * end);
        t += period;
    }
    let Some(path) = pb.finish() else {
        return;
    };
    let mut paint = Paint::default();
    paint.set_color_rgba8(255, 255, 255, 200);
    let stroke = Stroke {
        width: ui.f(1.0),
        ..Default::default()
    };
    pixmap.stroke_path(&path, &paint, &stroke, Transform::identity(), None);
}
