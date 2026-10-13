// SPDX-License-Identifier: GPL-3.0-only
//! 工具栏动作图标（EDT-7/PIN-1/M4/OUT-1/OUT-2）：撤销重做、贴图、长截图、复制、保存。
//! 从 `icons` 拆出，保证单文件不超过 300 行。

use tiny_skia::{PathBuilder, Pixmap, Rect, Transform};

use crate::overlay::SelRect;

use super::paint_of;

/// 撤销/重做图标：顶部半圆弧 + 端点三角（`mirror` 为 true 时镜像成重做）。
pub(crate) fn draw_undo_icon(pixmap: &mut Pixmap, r: SelRect, color: [u8; 4], mirror: bool) {
    let (cx, cy) = (r.x as f32 + r.w as f32 / 2.0, r.y as f32 + r.h as f32 / 2.0);
    let rad = 6.5;
    let flip = |x: f32| if mirror { 2.0 * cx - x } else { x };

    // 顶弧：θ 从 0°（右）扫到 180°（左）；y 轴向下，故取负
    let mut pb = PathBuilder::new();
    let steps = 18;
    for i in 0..=steps {
        let t = std::f32::consts::PI * i as f32 / steps as f32;
        let (x, y) = (flip(cx + rad * t.cos()), cy - rad * t.sin());
        if i == 0 {
            pb.move_to(x, y);
        } else {
            pb.line_to(x, y);
        }
    }
    if let Some(path) = pb.finish() {
        let stroke = tiny_skia::Stroke {
            width: 2.0,
            line_cap: tiny_skia::LineCap::Round,
            ..Default::default()
        };
        pixmap.stroke_path(&path, &paint_of(color), &stroke, Transform::identity(), None);
    }

    // 弧末端（左侧）向下的箭头三角
    let tip_x = flip(cx - rad);
    let h = 4.5;
    let mut tri = PathBuilder::new();
    tri.move_to(tip_x, cy + h);
    tri.line_to(tip_x - h * 0.75, cy - h * 0.55);
    tri.line_to(tip_x + h * 0.75, cy - h * 0.55);
    tri.close();
    if let Some(path) = tri.finish() {
        pixmap.fill_path(
            &path,
            &paint_of(color),
            tiny_skia::FillRule::Winding,
            Transform::identity(),
            None,
        );
    }
}

/// 贴图按钮图标（PIN-1，M2b）：图钉（圆头 + 杆 + 底部锥形）。
pub(crate) fn pin_icon(pixmap: &mut Pixmap, r: SelRect, color: [u8; 4]) {
    let paint = paint_of(color);
    let cx = r.x as f32 + r.w as f32 / 2.0;
    let head_y = r.y as f32 + 9.0;
    let head_r = 3.4;
    // 圆头
    let mut head = PathBuilder::new();
    head.push_circle(cx, head_y, head_r);
    if let Some(path) = head.finish() {
        pixmap.fill_path(&path, &paint, tiny_skia::FillRule::Winding, Transform::identity(), None);
    }
    // 杆（头到底部锥之间的竖线）
    let stroke = tiny_skia::Stroke {
        width: 1.8,
        line_cap: tiny_skia::LineCap::Round,
        ..Default::default()
    };
    let mut shaft = PathBuilder::new();
    shaft.move_to(cx, head_y + head_r);
    shaft.line_to(cx, (r.y + r.h) as f32 - 9.0);
    if let Some(path) = shaft.finish() {
        pixmap.stroke_path(&path, &paint, &stroke, Transform::identity(), None);
    }
    // 底部锥形（倒三角）
    let base_y = (r.y + r.h) as f32 - 6.5;
    let mut tri = PathBuilder::new();
    tri.move_to(cx - 4.2, base_y - 2.8);
    tri.line_to(cx + 4.2, base_y - 2.8);
    tri.line_to(cx, base_y + 2.8);
    tri.close();
    if let Some(path) = tri.finish() {
        pixmap.fill_path(&path, &paint, tiny_skia::FillRule::Winding, Transform::identity(), None);
    }
}

/// 置顶 / 置底图标（EDT-7 层级调整）：方向箭头 + 基座横线。
pub(crate) fn layer_icon(pixmap: &mut Pixmap, r: SelRect, color: [u8; 4], up: bool) {
    let paint = paint_of(color);
    let cx = r.x as f32 + r.w as f32 / 2.0;
    let (base_y, tip_y) = if up {
        (r.y as f32 + 18.0, r.y as f32 + 7.0)
    } else {
        (r.y as f32 + 8.0, r.y as f32 + 19.0)
    };
    let stroke = tiny_skia::Stroke {
        width: 2.0,
        line_cap: tiny_skia::LineCap::Round,
        ..Default::default()
    };
    // 竖线
    let mut shaft = PathBuilder::new();
    shaft.move_to(cx, base_y);
    shaft.line_to(cx, tip_y + if up { 4.0 } else { -4.0 });
    if let Some(path) = shaft.finish() {
        pixmap.stroke_path(&path, &paint, &stroke, Transform::identity(), None);
    }
    // 箭头三角
    let mut tri = PathBuilder::new();
    tri.move_to(cx, tip_y);
    tri.line_to(cx - 4.0, tip_y + if up { 5.0 } else { -5.0 });
    tri.line_to(cx + 4.0, tip_y + if up { 5.0 } else { -5.0 });
    tri.close();
    if let Some(path) = tri.finish() {
        pixmap.fill_path(&path, &paint, tiny_skia::FillRule::Winding, Transform::identity(), None);
    }
    // 基座横线（表示叠放面）
    let mut base = PathBuilder::new();
    base.move_to(r.x as f32 + 6.0, base_y);
    base.line_to((r.x + r.w) as f32 - 6.0, base_y);
    if let Some(path) = base.finish() {
        pixmap.stroke_path(&path, &paint, &stroke, Transform::identity(), None);
    }
}

/// 删除图标（EDT-7）：垃圾桶（盖子 + 提手 + 桶身）。
pub(crate) fn trash_icon(pixmap: &mut Pixmap, r: SelRect, color: [u8; 4]) {
    let paint = paint_of(color);
    let stroke = tiny_skia::Stroke {
        width: 1.5,
        line_cap: tiny_skia::LineCap::Round,
        ..Default::default()
    };
    let (x0, x1) = (r.x as f32 + 6.0, (r.x + r.w) as f32 - 6.0);
    let lid_y = r.y as f32 + 8.0;
    // 盖子
    let mut lid = PathBuilder::new();
    lid.move_to(x0, lid_y);
    lid.line_to(x1, lid_y);
    if let Some(path) = lid.finish() {
        pixmap.stroke_path(&path, &paint, &stroke, Transform::identity(), None);
    }
    // 提手
    let mut handle = PathBuilder::new();
    handle.move_to(x0 + 4.0, lid_y - 3.0);
    handle.line_to(x1 - 4.0, lid_y - 3.0);
    if let Some(path) = handle.finish() {
        pixmap.stroke_path(&path, &paint, &stroke, Transform::identity(), None);
    }
    // 桶身（上宽下窄的梯形）
    let (ty, by) = (lid_y + 3.0, (r.y + r.h) as f32 - 5.0);
    let mut body = PathBuilder::new();
    body.move_to(x0 + 1.5, ty);
    body.line_to(x1 - 1.5, ty);
    body.line_to(x1 - 3.0, by);
    body.line_to(x0 + 3.0, by);
    body.close();
    if let Some(path) = body.finish() {
        pixmap.stroke_path(&path, &paint, &stroke, Transform::identity(), None);
    }
}

/// 长截图按钮图标（M4）：上下两层页面 + 向下箭头，表示纵向延伸拼接。
pub(crate) fn longshot_icon(pixmap: &mut Pixmap, r: SelRect, color: [u8; 4]) {
    let paint = paint_of(color);
    // 上层页面（小矩形）
    let top = Rect::from_xywh(r.x as f32 + 6.0, r.y as f32 + 5.0, 12.0, 9.0);
    if let Some(top) = top {
        let mut pb = PathBuilder::new();
        pb.push_rect(top);
        if let Some(path) = pb.finish() {
            pixmap.fill_path(&path, &paint, tiny_skia::FillRule::Winding, Transform::identity(), None);
        }
    }
    // 下层页面（大矩形）
    let bottom = Rect::from_xywh(r.x as f32 + 6.0, r.y as f32 + 13.0, 14.0, 8.0);
    if let Some(bottom) = bottom {
        let mut pb = PathBuilder::new();
        pb.push_rect(bottom);
        if let Some(path) = pb.finish() {
            let stroke = tiny_skia::Stroke {
                width: 1.6,
                ..Default::default()
            };
            pixmap.stroke_path(&path, &paint, &stroke, Transform::identity(), None);
        }
    }
    // 向下箭头（页面下方）
    let (ax, ay) = (r.x as f32 + 13.0, r.y as f32 + 21.0);
    let mut tri = PathBuilder::new();
    tri.move_to(ax - 3.5, ay - 2.5);
    tri.line_to(ax + 3.5, ay - 2.5);
    tri.line_to(ax, ay + 2.5);
    tri.close();
    if let Some(path) = tri.finish() {
        pixmap.fill_path(&path, &paint, tiny_skia::FillRule::Winding, Transform::identity(), None);
    }
}

/// 复制按钮图标（OUT-1）：两张叠放的纸（后纸右上、前纸左下，均描边）。
pub(crate) fn copy_icon(pixmap: &mut Pixmap, r: SelRect, color: [u8; 4]) {
    let paint = paint_of(color);
    let stroke = tiny_skia::Stroke {
        width: 1.4,
        ..Default::default()
    };
    let papers = [
        Rect::from_xywh(r.x as f32 + 10.0, r.y as f32 + 5.0, 10.0, 12.0),
        Rect::from_xywh(r.x as f32 + 6.0, r.y as f32 + 9.0, 10.0, 12.0),
    ];
    for rect in papers.into_iter().flatten() {
        let mut pb = PathBuilder::new();
        pb.push_rect(rect);
        if let Some(path) = pb.finish() {
            pixmap.stroke_path(&path, &paint, &stroke, Transform::identity(), None);
        }
    }
}

/// 保存按钮图标（OUT-2）：软盘（外框右上斜切 + 上部滑片 + 底部标签）。
pub(crate) fn save_icon(pixmap: &mut Pixmap, r: SelRect, color: [u8; 4]) {
    let paint = paint_of(color);
    let x = r.x as f32 + 6.0;
    let y = r.y as f32 + 5.0;
    let (w, h) = (14.0, 16.0);
    let stroke = tiny_skia::Stroke {
        width: 1.4,
        ..Default::default()
    };
    let mut outer = PathBuilder::new();
    outer.move_to(x, y);
    outer.line_to(x + w - 3.0, y);
    outer.line_to(x + w, y + 3.0);
    outer.line_to(x + w, y + h);
    outer.line_to(x, y + h);
    outer.close();
    if let Some(path) = outer.finish() {
        pixmap.stroke_path(&path, &paint, &stroke, Transform::identity(), None);
    }
    // 上部滑片 + 底部标签（实心块）
    for tag in [
        Rect::from_xywh(x + 3.0, y, 8.0, 6.0),
        Rect::from_xywh(x + 3.0, y + 10.0, 8.0, 6.0),
    ]
    .into_iter()
    .flatten()
    {
        let mut pb = PathBuilder::new();
        pb.push_rect(tag);
        if let Some(path) = pb.finish() {
            pixmap.fill_path(&path, &paint, tiny_skia::FillRule::Winding, Transform::identity(), None);
        }
    }
}
