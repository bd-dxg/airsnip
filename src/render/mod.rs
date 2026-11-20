// SPDX-License-Identifier: GPL-3.0-only
//! 矢量渲染（PRD 6.3.2/6.3.3）：把标注对象绘制到 tiny-skia 像素图上。
//! 只负责「对象 → 像素」，不含上屏与脏区逻辑（见 `crate::overlay`）。

use crate::editor::{
    Bounds, Kind, Object, Point, Style, arrow_head_len, end_tangent, flatten, head_entry_t, polyline_len, split_left,
    t_at_arc_from_end,
};
use tiny_skia::{FillRule, LineCap, LineJoin, Path, PathBuilder, Pixmap, Rect, Stroke, StrokeDash, Transform};

mod badge;
pub use badge::number_radius;
pub(crate) use badge::{badge_preview, contrast_ink};

mod bake;
pub use bake::bake_objects;
mod blur;
pub mod fonts;
mod gradient_stroke;
pub mod icon;
pub(crate) mod mask;
mod paint;
pub(crate) use paint::{box_span, paint_of, solid_paint};
mod region;
mod stroke;
pub mod text;
mod text_bitmap;

/// 像素区域（x0, y0, x1, y1 半开区间，物理像素）。
pub type Clip = (i32, i32, i32, i32);

/// 绘制对象列表；`draft` 是正在拖拽的临时对象，绘制在最上层。
/// `skip` 为需隐藏的对象索引（二次编辑中的文本对象由输入框呈现）。
/// 只有与 `clip` 相交的对象会被绘制，调用方需保证 `clip` 已包含它们的包围盒（否则会污染区域外的像素）。
pub fn draw_objects(pixmap: &mut Pixmap, objects: &[Object], draft: Option<&Object>, skip: Option<usize>, clip: Clip) {
    for (i, obj) in objects.iter().enumerate() {
        if Some(i) != skip && intersects(&obj.bounds(), clip) {
            draw_object(pixmap, obj);
        }
    }
    if let Some(draft) = draft
        && intersects(&draft.bounds(), clip)
    {
        draw_object(pixmap, draft);
    }
}

/// 绘制单个对象。
pub fn draw_object(pixmap: &mut Pixmap, obj: &Object) {
    draw_object_with(pixmap, obj, Transform::identity());
}

/// 绘制单个对象，并施加坐标系变换（导出烘焙时用于平移到裁剪图坐标）。
pub fn draw_object_with(pixmap: &mut Pixmap, obj: &Object, transform: Transform) {
    match &obj.kind {
        Kind::Rect { a, b, radius, filled } => draw_rect(pixmap, *a, *b, *radius, *filled, &obj.style, transform),
        Kind::Arrow { from, c1, c2, to } => draw_arrow(pixmap, *from, *c1, *c2, *to, &obj.style, transform),
        Kind::Text {
            pos,
            text,
            font_size,
            font_family,
            bold,
            italic,
            ..
        } => text::draw(
            pixmap,
            *pos,
            text,
            &text::FontSpec::from_parts(font_family, *font_size, *bold, *italic),
            obj.style.color,
            transform,
        ),
        Kind::Highlight { a, b, corner } => region::draw_highlight(pixmap, *a, *b, *corner, &obj.style, transform),
        Kind::Blur { a, b, radius, corner } => region::draw_blur(pixmap, *a, *b, *radius, *corner, transform),
        // 聚光高亮：不在底图上涂色，效果由「选区暗化 + 区域恢复原图亮度」的渲染管线提供
        // （屏幕显示见 overlay::selection::repaint，导出见 bake_objects）
        Kind::Glow { .. } => {}
        // EDT-12 序号标记：徽标圆 + 引线箭头 + 反白数字
        Kind::Number { .. } => badge::draw_number(pixmap, obj, transform),
        // EDT-13 直线：无箭头纯线段（圆端帽）
        Kind::Line { from, to } => stroke::draw_line(pixmap, *from, *to, &obj.style, transform),
        // EDT-14 椭圆：描边 + 可选填充（与矩形同套路）
        Kind::Ellipse { a, b, filled } => stroke::draw_ellipse(pixmap, *a, *b, *filled, &obj.style, transform),
        // EDT-15 手绘：中点细分 + 二次贝塞尔平滑描边
        Kind::Freehand { pts } => stroke::draw_freehand(pixmap, pts.as_slice(), &obj.style, transform),
    }
}

/// 导出烘焙见 `bake` 子模块（bake_objects）。

/// 对象包围盒是否与像素区域相交。
pub fn intersects(b: &Bounds, clip: Clip) -> bool {
    b.x0 < clip.2 as f32 && b.x1 > clip.0 as f32 && b.y0 < clip.3 as f32 && b.y1 > clip.1 as f32
}

/// 对象包围盒向外取整为像素区域，供脏区计算。
pub fn bounds_to_clip(b: &Bounds) -> Clip {
    (
        b.x0.floor() as i32,
        b.y0.floor() as i32,
        b.x1.ceil() as i32 + 1,
        b.y1.ceil() as i32 + 1,
    )
}

/// 荧光笔透明度（HIGHLIGHT_ALPHA）与高亮/模糊绘制见 `region`（EDT-5/EDT-6）。
/// 描边参数（EDT-18）：线型按 `style.line` 换成 tiny-skia 的 dash 数组；
/// 虚线一律用平头端点（圆头会把每段包成胶囊、间隙视觉上缩一半，ADR 0042）。
pub(super) fn stroke_of(style: &Style, cap: LineCap, join: LineJoin) -> Stroke {
    let dash = style.line.dash_array(style.width).and_then(|a| StrokeDash::new(a, 0.0));
    Stroke {
        width: style.width,
        line_cap: if dash.is_some() { LineCap::Butt } else { cap },
        line_join: join,
        dash,
        ..Default::default()
    }
}

/// EDT-1 矩形：描边 + 可选填充、圆角、透明度（由颜色 alpha 控制）。
fn draw_rect(pixmap: &mut Pixmap, a: Point, b: Point, radius: f32, filled: bool, style: &Style, tr: Transform) {
    let (x0, y0) = (a.x.min(b.x), a.y.min(b.y));
    let (x1, y1) = (a.x.max(b.x), a.y.max(b.y));
    if x1 - x0 < 1.0 || y1 - y0 < 1.0 {
        return;
    }
    let Some(path) = rounded_rect_path(x0, y0, x1, y1, radius) else {
        return;
    };
    let (ga, gb) = box_span(x0, y0, x1, y1);
    let paint = paint_of(style, ga, gb);

    if filled {
        // 填充用半透明色（与描边同色），透明度由颜色 alpha 表达
        pixmap.fill_path(&path, &paint, FillRule::Winding, tr, None);
        return;
    }
    // 渐变描边沿**周长弧长**走色（否则水平渐变下矩形的上边与下边同色，ADR 0043）
    if style.is_gradient() {
        // 直角矩形 + 实线：4 条边各一次渐变，比沿周长切段快一个数量级。
        // 虚线 / 点划线必须走分段：`stroke_rect_edges` 用 Square 端帽填角，会把虚线间隔补成实线。
        if radius < 0.5 && style.line.is_solid() {
            gradient_stroke::stroke_rect_edges(pixmap, (x0, y0, x1, y1), style, tr);
        } else {
            gradient_stroke::stroke_gradient(pixmap, &path, style, tr);
        }
        return;
    }
    let stroke = stroke_of(style, LineCap::Butt, LineJoin::Miter);
    pixmap.stroke_path(&path, &paint, &stroke, tr, None);
}

/// 圆角矩形路径，圆角用三次贝塞尔近似圆弧；`radius <= 0.5` 时退化为直角矩形。
fn rounded_rect_path(x0: f32, y0: f32, x1: f32, y1: f32, radius: f32) -> Option<Path> {
    let r = radius.min((x1 - x0) * 0.5).min((y1 - y0) * 0.5).max(0.0);
    let mut pb = PathBuilder::new();
    if r < 0.5 {
        pb.push_rect(Rect::from_ltrb(x0, y0, x1, y1)?);
        return pb.finish();
    }
    let k = r * 0.552_284_8;
    pb.move_to(x0 + r, y0);
    pb.line_to(x1 - r, y0);
    pb.cubic_to(x1 - r + k, y0, x1, y0 + r - k, x1, y0 + r);
    pb.line_to(x1, y1 - r);
    pb.cubic_to(x1, y1 - r + k, x1 - r + k, y1, x1 - r, y1);
    pb.line_to(x0 + r, y1);
    pb.cubic_to(x0 + r - k, y1, x0, y1 - r + k, x0, y1 - r);
    pb.line_to(x0, y0 + r);
    pb.cubic_to(x0, y0 + r - k, x0 + r - k, y0, x0 + r, y0);
    pb.close();
    pb.finish()
}

/// EDT-2/EDT-3 箭头：线身沿三次贝塞尔曲线，头部沿终点切线方向；
/// 线身在头部底边处按弧长截断，避免粗线从箭尖穿出（EDT-3a）。
fn draw_arrow(pixmap: &mut Pixmap, from: Point, c1: Point, c2: Point, to: Point, style: &Style, tr: Transform) {
    let pts = flatten(from, c1, c2, to);
    let total = polyline_len(&pts);
    if total < 0.5 {
        return;
    }
    let (ux, uy) = unit_dir(c1, c2, to, &pts);
    let head = arrow_head_len(style.width).min(total);
    let half = head * 0.45;
    let base = Point::new(to.x - ux * head, to.y - uy * head);
    let paint = paint_of(style, from, to);

    // 线身：截断在头部三角的边界（底边或侧边）上，避免弯曲时与头部脱开或穿出三角侧面（EDT-3a）；
    // 末端沿曲线在截断点的切线方向再深入「线宽一半 + 1px」，保证粗线也能完全没入三角，
    // 不留下接缝（细线时代的固定 0.5px 深入对粗线不足）。
    let t0 =
        head_entry_t(from, c1, c2, to, base, (ux, uy), head, half).unwrap_or_else(|| t_at_arc_from_end(&pts, head));
    let seg = split_left(from, c1, c2, to, t0);
    let (tx, ty) = {
        // 截断点处曲线的切线方向（切线与箭头方向不一致时，沿切线深入才不会滑出三角）
        let (dx, dy) = (seg[3].x - seg[2].x, seg[3].y - seg[2].y);
        let len = (dx * dx + dy * dy).sqrt();
        if len > 1e-6 { (dx / len, dy / len) } else { (ux, uy) }
    };
    let deep = (style.width * 0.5 + 1.0).max(1.0);
    let line_end = Point::new(seg[3].x + tx * deep, seg[3].y + ty * deep);
    let mut line = PathBuilder::new();
    line.move_to(from.x, from.y);
    line.cubic_to(seg[1].x, seg[1].y, seg[2].x, seg[2].y, line_end.x, line_end.y);
    if let Some(path) = line.finish() {
        let stroke = stroke_of(style, LineCap::Butt, LineJoin::Miter);
        pixmap.stroke_path(&path, &paint, &stroke, tr, None);
    }

    // 箭头三角（填充），底边垂直于终点切线
    let (px, py) = (-uy, ux);
    let mut tri = PathBuilder::new();
    tri.move_to(to.x, to.y);
    tri.line_to(base.x + px * half, base.y + py * half);
    tri.line_to(base.x - px * half, base.y - py * half);
    tri.close();
    if let Some(path) = tri.finish() {
        pixmap.fill_path(&path, &paint, FillRule::Winding, tr, None);
    }
}

/// 终点切线方向的单位向量（EDT-3：P3−P2，退化时回退 P3−P1 或折线末段）。
fn unit_dir(c1: Point, c2: Point, to: Point, pts: &[Point]) -> (f32, f32) {
    let t = end_tangent(c1, c2, to);
    if let Some(u) = normalize(t.x, t.y) {
        return u;
    }
    let prev = pts[pts.len() - 2];
    normalize(to.x - prev.x, to.y - prev.y).unwrap_or((1.0, 0.0))
}

fn normalize(x: f32, y: f32) -> Option<(f32, f32)> {
    let len = (x * x + y * y).sqrt();
    if len < 1e-6 { None } else { Some((x / len, y / len)) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::editor::{Kind, Style};
    use tiny_skia::Paint;

    fn new_pixmap(w: u32, h: u32) -> Pixmap {
        let mut p = Pixmap::new(w, h).unwrap();
        p.fill(tiny_skia::Color::TRANSPARENT);
        p
    }

    /// 某一行上「连续有墨」的段数（虚线判据：有空隙就会多于一段）。
    fn ink_runs(pm: &Pixmap, y: u32) -> usize {
        let mut runs = 0;
        let mut prev = false;
        for x in 0..pm.width() {
            let ink = pm.pixel(x, y).is_some_and(|p| p.alpha() > 0);
            if ink && !prev {
                runs += 1;
            }
            prev = ink;
        }
        runs
    }

    /// EDT-18：虚线真的切开了——同一条 8px 粗直线，实线连续一段、虚线 / 点划线分成多段。
    #[test]
    fn dashed_stroke_has_gaps() {
        let style = Style::new([255, 0, 0, 255], 8.0);
        let (a, b) = (Point::new(10.0, 20.0), Point::new(190.0, 20.0));
        let mut solid = new_pixmap(200, 40);
        stroke::draw_line(&mut solid, a, b, &style, Transform::identity());
        assert_eq!(ink_runs(&solid, 20), 1, "实线应是一整段");

        let mut dashed = new_pixmap(200, 40);
        let style = style.with_line(crate::editor::LineStyle::Dashed);
        stroke::draw_line(&mut dashed, a, b, &style, Transform::identity());
        let runs = ink_runs(&dashed, 20);
        assert!(runs >= 4, "虚线应分成多段，实际 {runs} 段");

        let mut dot = new_pixmap(200, 40);
        let style = style.with_line(crate::editor::LineStyle::DashDot);
        stroke::draw_line(&mut dot, a, b, &style, Transform::identity());
        assert!(ink_runs(&dot, 20) >= 3, "点划线同样应有空隙");
    }

    /// EDT-19：渐变填充真的从起色走到止色（起 / 中 / 止三点互不相同，中间提亮）。
    #[test]
    fn gradient_fill_runs_between_stops() {
        let g = crate::editor::Gradient::seed([229, 57, 53, 255]);
        let style = Style::new(g.representative(), 4.0).with_gradient(g);
        let obj = Object {
            kind: Kind::Rect {
                a: Point::new(10.0, 10.0),
                b: Point::new(190.0, 90.0),
                radius: 0.0,
                filled: true,
            },
            style,
        };
        let mut pm = new_pixmap(200, 100);
        draw_object(&mut pm, &obj);
        let px = |x: u32| pm.pixel(x, 50).map(|p| (p.red(), p.green(), p.blue()));
        let (left, mid, right) = (px(20).unwrap(), px(100).unwrap(), px(180).unwrap());
        assert_ne!(left, right, "两端颜色应不同（渐变真的存在）");
        assert_ne!(left, mid, "中间不应与起点同色");
        assert_ne!(mid, right, "中间不应与终点同色");
        // 一套渐变里要能数出多种颜色：在 1/4、1/2、3/4 处各取一点，都不能相同
        let q = |f: f32| px((10.0 + 180.0 * f) as u32).unwrap();
        let (a, b, c) = (q(0.25), q(0.5), q(0.75));
        assert_ne!(a, b, "1/4 与 1/2 应不同：{a:?} vs {b:?}");
        assert_ne!(b, c, "1/2 与 3/4 应不同：{b:?} vs {c:?}");
    }

    /// EDT-19：方向取长边——宽矩形是水平渐变（同一列上下同色、左右不同色）。
    #[test]
    fn gradient_direction_follows_long_edge() {
        let g = crate::editor::Gradient::seed([229, 57, 53, 255]);
        let style = Style::new(g.representative(), 4.0).with_gradient(g);
        let wide = Object {
            kind: Kind::Rect {
                a: Point::new(10.0, 10.0),
                b: Point::new(190.0, 90.0),
                radius: 0.0,
                filled: true,
            },
            style,
        };
        let mut pm = new_pixmap(200, 100);
        draw_object(&mut pm, &wide);
        let rgb = |x: u32, y: u32| pm.pixel(x, y).map(|p| (p.red(), p.green(), p.blue()));
        let (left, right) = (rgb(20, 50).unwrap(), rgb(180, 50).unwrap());
        assert_ne!(left, right, "左右应不同（水平渐变）");
        let (top, bottom) = (rgb(100, 20).unwrap(), rgb(100, 80).unwrap());
        assert_eq!(top, bottom, "水平渐变上下应同色");
    }

    /// EDT-19：未填充矩形只画描边，描边也照样跟渐变（上边框两端颜色不同）。
    #[test]
    fn gradient_stroke_varies_along_edge() {
        let g = crate::editor::Gradient::seed([30, 136, 229, 255]);
        let style = Style::new(g.representative(), 6.0).with_gradient(g);
        let obj = Object {
            kind: Kind::Rect {
                a: Point::new(10.0, 10.0),
                b: Point::new(190.0, 90.0),
                radius: 0.0,
                filled: false,
            },
            style,
        };
        let mut pm = new_pixmap(200, 100);
        draw_object(&mut pm, &obj);
        let left = pm.pixel(20, 12).map(|p| (p.red(), p.green(), p.blue()));
        let right = pm.pixel(180, 12).map(|p| (p.red(), p.green(), p.blue()));
        assert!(left.is_some() && right.is_some(), "上下边框应有墨");
        assert_ne!(left, right, "描边也应跟着渐变变化");
    }

    /// EDT-19：渐变 + 虚线可以共存（两个独立维度），描边不 panic 且有墨。
    #[test]
    fn gradient_and_dashed_can_combine() {
        let g = crate::editor::Gradient::seed([229, 57, 53, 255]);
        let style = Style::new(g.representative(), 6.0)
            .with_line(crate::editor::LineStyle::Dashed)
            .with_gradient(g);
        let obj = Object {
            kind: Kind::arrow(Point::new(10.0, 30.0), Point::new(180.0, 30.0)),
            style,
        };
        let mut pm = new_pixmap(200, 60);
        draw_object(&mut pm, &obj);
        // 箭头线身 + 三角头都要有墨，且经过渐变着色器（不崩、不是全透明）
        let ink = (10..180)
            .filter(|x| pm.pixel(*x as u32, 30).is_some_and(|p| p.alpha() > 0))
            .count();
        assert!(ink > 0, "虚线渐变箭头应有墨");
    }

    fn alpha_at(p: &Pixmap, x: u32, y: u32) -> u8 {
        p.data()[((y * p.width() + x) * 4 + 3) as usize]
    }

    #[test]
    fn arrow_line_is_drawn_between_endpoints() {
        let mut p = new_pixmap(200, 60);
        let obj = Object {
            kind: Kind::arrow(Point::new(10.0, 30.0), Point::new(180.0, 30.0)),
            style: Style::new([255, 0, 0, 255], 4.0),
        };
        draw_object(&mut p, &obj);

        assert!(alpha_at(&p, 50, 30) > 200, "线身中心应有像素");
        assert!(alpha_at(&p, 10, 30) > 200, "起点应有像素");
        assert_eq!(alpha_at(&p, 5, 5), 0, "远离箭头的区域应保持透明");
    }

    #[test]
    fn arrow_head_scales_with_width() {
        let head_thin = arrow_head_len(2.0);
        let head_thick = arrow_head_len(8.0);
        assert!(head_thick > head_thin * 2.0);

        // 粗箭头在尖端附近的垂直覆盖范围应更大
        let mut thin = new_pixmap(200, 120);
        let mut thick = new_pixmap(200, 120);
        let mk = |w: f32| Object {
            kind: Kind::arrow(Point::new(10.0, 60.0), Point::new(180.0, 60.0)),
            style: Style::new([0, 0, 0, 255], w),
        };
        draw_object(&mut thin, &mk(2.0));
        draw_object(&mut thick, &mk(8.0));

        // 三角形头部的角度相同（按线宽等比缩放），因此比较「最宽处」而非固定列
        let wide = |p: &Pixmap| {
            (0..200)
                .map(|x| (0..120).filter(|y| alpha_at(p, x, *y) > 32).count())
                .max()
                .unwrap_or(0)
        };
        assert!(
            wide(&thick) > wide(&thin) * 2,
            "粗箭头的头部应明显更宽：thin={} thick={}",
            wide(&thin),
            wide(&thick)
        );
    }

    #[test]
    fn rounded_rect_stroke_covers_corners_but_not_center() {
        let mut p = new_pixmap(120, 120);
        let obj = Object {
            kind: Kind::Rect {
                a: Point::new(10.0, 10.0),
                b: Point::new(110.0, 110.0),
                radius: 12.0,
                filled: false,
            },
            style: Style::new([0, 0, 255, 255], 3.0),
        };
        draw_object(&mut p, &obj);

        assert!(alpha_at(&p, 10, 60) > 128, "左边框应有像素");
        assert!(alpha_at(&p, 60, 10) > 128, "上边框应有像素");
        assert_eq!(alpha_at(&p, 60, 60), 0, "未填充的矩形内部应保持透明");
        assert_eq!(alpha_at(&p, 10, 10), 0, "圆角处（r=12）应被切掉");
    }

    #[test]
    fn filled_rect_covers_interior_with_alpha() {
        let mut p = new_pixmap(120, 120);
        let obj = Object {
            kind: Kind::Rect {
                a: Point::new(10.0, 10.0),
                b: Point::new(110.0, 110.0),
                radius: 0.0,
                filled: true,
            },
            style: Style::new([0, 200, 0, 60], 2.0), // 半透明填充（EDT-1 透明度）
        };
        draw_object(&mut p, &obj);

        let a = alpha_at(&p, 60, 60);
        assert!((50..=70).contains(&a), "内部应为半透明填充，实际 alpha={a}");
    }

    #[test]
    fn bake_translates_objects_into_crop_coordinates() {
        let mut p = new_pixmap(300, 300);
        let obj = Object {
            kind: Kind::Rect {
                a: Point::new(100.0, 100.0),
                b: Point::new(160.0, 160.0),
                radius: 0.0,
                filled: false,
            },
            style: Style::new([255, 0, 0, 255], 4.0),
        };
        // 裁剪区域左上角在覆盖层坐标 (100,100)：对象边框应落在裁剪图坐标 0 附近
        bake_objects(&mut p, &[obj], (100, 100));

        assert!(alpha_at(&p, 1, 30) > 128, "左边框应平移到裁剪图 x≈0");
        assert!(alpha_at(&p, 30, 1) > 128, "上边框应平移到裁剪图 y≈0");
        assert_eq!(alpha_at(&p, 101, 101), 0, "原坐标处不应有像素");
    }

    #[test]
    fn bake_draws_nothing_without_objects() {
        let mut p = new_pixmap(20, 20);
        bake_objects(&mut p, &[], (0, 0));
        assert!(p.data().iter().all(|b| *b == 0));
    }

    #[test]
    fn curved_arrow_follows_control_points() {
        let mut p = new_pixmap(200, 200);
        let obj = Object {
            // 起点 (20,180) → 终点 (180,180)，控制点抬到 y=40：曲线应向上拱起
            kind: Kind::Arrow {
                from: Point::new(20.0, 180.0),
                c1: Point::new(20.0, 40.0),
                c2: Point::new(180.0, 40.0),
                to: Point::new(180.0, 180.0),
            },
            style: Style::new([255, 0, 0, 255], 3.0),
        };
        draw_object(&mut p, &obj);

        // 三次贝塞尔在 t=0.5 处为 (100, 75)
        assert!(alpha_at(&p, 100, 75) > 128, "曲线中段应经过 (100,75)");
        assert_eq!(alpha_at(&p, 100, 180), 0, "两点连线上不应有像素");
        // 箭尖是个尖点（零面积），取头部内部靠后的位置校验
        assert!(alpha_at(&p, 180, 172) > 128, "头部内部应有像素");
    }

    #[test]
    fn curve_head_points_along_end_tangent() {
        // 终点切线 ≈ P3−P2 = (10,−90)（几乎竖直向上）：头部应向上展开，不向左侧展开
        let mut p = new_pixmap(200, 200);
        let obj = Object {
            kind: Kind::Arrow {
                from: Point::new(40.0, 40.0),
                c1: Point::new(40.0, 140.0),
                c2: Point::new(150.0, 150.0),
                to: Point::new(160.0, 60.0),
            },
            style: Style::new([0, 0, 0, 255], 4.0),
        };
        draw_object(&mut p, &obj);

        assert!(alpha_at(&p, 96, 121) > 128, "线身应经过 t=0.5 处 (96,121)");
        assert!(alpha_at(&p, 159, 66) > 128, "头部应沿切线（向上）展开");
        assert_eq!(alpha_at(&p, 152, 60), 0, "头部不应沿水平方向展开");
    }

    #[test]
    fn clip_skips_objects_outside_region() {
        let b = Object {
            kind: Kind::Rect {
                a: Point::new(10.0, 10.0),
                b: Point::new(20.0, 20.0),
                radius: 0.0,
                filled: false,
            },
            style: Style::new([0, 0, 0, 255], 2.0),
        };
        assert!(intersects(&b.bounds(), (0, 0, 100, 100)));
        assert!(!intersects(&b.bounds(), (200, 200, 300, 300)));
    }

    /// 曲线箭头：沿「尖端 → 线身」方向取横截面，任何截面都不能出现断裂（露背景色）。
    #[test]
    fn curved_arrow_head_attached_at_wide_width() {
        let (from, c1, c2, to) = (
            Point::new(30.0, 260.0),
            Point::new(40.0, 60.0),
            Point::new(260.0, 40.0),
            Point::new(270.0, 250.0),
        );
        for w in [4.0, 24.0] {
            let mut p = new_pixmap(300, 300);
            p.fill(tiny_skia::Color::from_rgba8(40, 40, 40, 255));
            let obj = Object {
                kind: Kind::Arrow { from, c1, c2, to },
                style: Style::new([255, 0, 0, 255], w),
            };
            draw_object(&mut p, &obj);

            // 沿终点切线方向从尖端往回扫描：横截面必须始终有红色覆盖，无断裂
            // 扫描到 base（head）往后 60px，覆盖线身与头部交界处
            let t = end_tangent(c1, c2, to);
            let len = (t.x * t.x + t.y * t.y).sqrt();
            let (ux, uy) = (t.x / len, t.y / len);
            let perp = (-uy, ux);
            let span = (w * 0.5 + 4.0) as i32;
            let head = arrow_head_len(w);
            let max_back = (head + 60.0) as i32;
            for s in (0..=max_back).rev() {
                let back = s as f32; // 距尖端 0..(head+60)px
                let (cx, cy) = (to.x - ux * back, to.y - uy * back);
                let mut covered = 0; // 覆盖线宽范围的横截面上红色像素数
                for o in -span..=span {
                    let px = (cx + perp.0 * o as f32).round() as i32;
                    let py = (cy + perp.1 * o as f32).round() as i32;
                    if px >= 0 && px < 300 && py >= 0 && py < 300 && alpha_at(&p, px as u32, py as u32) > 0 {
                        covered += 1;
                    }
                }
                let inside = back <= head + 0.5; // 三角内截面必然实心
                let expect = if inside { (w * 0.5) as i32 } else { 1 };
                assert!(
                    covered >= expect,
                    "w={w} 距尖端 {back:.1}px 处断面覆盖过少（脱节）：covered={covered}"
                );
            }
        }
    }

    /// EDT-13/14/15：直线、椭圆（描边 / 填充）、手绘的像素覆盖。
    #[test]
    fn new_tools_paint_expected_pixels() {
        // 直线：线身中间有像素，线上方无像素
        let mut p = new_pixmap(200, 60);
        let line = Object {
            kind: Kind::Line {
                from: Point::new(10.0, 30.0),
                to: Point::new(180.0, 30.0),
            },
            style: Style::new([255, 0, 0, 255], 4.0),
        };
        draw_object(&mut p, &line);
        assert!(alpha_at(&p, 90, 30) > 150, "直线中心应有像素");
        assert_eq!(alpha_at(&p, 90, 5), 0, "直线外应保持透明");

        // 椭圆描边：右端与上端有像素，中心透明
        let mut e = new_pixmap(200, 120);
        let outline = Object {
            kind: Kind::Ellipse {
                a: Point::new(10.0, 10.0),
                b: Point::new(190.0, 110.0),
                filled: false,
            },
            style: Style::new([0, 0, 255, 255], 3.0),
        };
        draw_object(&mut e, &outline);
        assert!(alpha_at(&e, 190, 60) > 80, "椭圆右端应有像素");
        assert!(alpha_at(&e, 100, 11) > 80, "椭圆上端应有像素");
        assert_eq!(alpha_at(&e, 100, 60), 0, "未填充椭圆中心应透明");

        // 椭圆填充：中心不透明
        let mut ef = new_pixmap(200, 120);
        let filled = Object {
            kind: Kind::Ellipse {
                a: Point::new(10.0, 10.0),
                b: Point::new(190.0, 110.0),
                filled: true,
            },
            style: Style::new([0, 200, 0, 255], 2.0),
        };
        draw_object(&mut ef, &filled);
        assert!(alpha_at(&ef, 100, 60) > 150, "填充椭圆中心应有颜色");

        // 手绘：经顶点与折线中点，远离处透明
        let mut f = new_pixmap(200, 120);
        let stroke = Object {
            kind: Kind::Freehand {
                pts: std::sync::Arc::new(vec![
                    Point::new(20.0, 100.0),
                    Point::new(100.0, 20.0),
                    Point::new(180.0, 100.0),
                ]),
            },
            style: Style::new([255, 0, 0, 255], 4.0),
        };
        draw_object(&mut f, &stroke);
        // 中点平滑：首段直线到 (60,60)（必经过），二次曲线顶点在 (100,40)（非控制点 (100,20)）
        assert!(alpha_at(&f, 60, 60) > 40, "手绘首段中点附近应有像素");
        assert!(alpha_at(&f, 100, 40) > 40, "手绘曲线顶点附近应有像素");
        assert_eq!(alpha_at(&f, 100, 100), 0, "手绘下方应保持透明");
    }

    /// EDT-13/14/15：烘焙导出时坐标正确平移。
    #[test]
    fn new_tools_bake_with_translate() {
        let mut p = new_pixmap(100, 100);
        let line = Object {
            kind: Kind::Line {
                from: Point::new(20.0, 50.0),
                to: Point::new(80.0, 50.0),
            },
            style: Style::new([255, 0, 0, 255], 3.0),
        };
        // 裁剪原点 (10,10) → 线身落在裁剪图 (10,40)-(70,40)
        bake_objects(&mut p, &[line], (10, 10));
        assert!(alpha_at(&p, 40, 40) > 100, "平移后的线身应有像素");
        assert_eq!(alpha_at(&p, 60, 10), 0, "原坐标处不应有像素");
    }

    /// 读取像素 RGBA。
    fn pixel(p: &Pixmap, x: u32, y: u32) -> (u8, u8, u8, u8) {
        let off = ((y * p.width() + x) * 4) as usize;
        (p.data()[off], p.data()[off + 1], p.data()[off + 2], p.data()[off + 3])
    }

    #[test]
    fn highlight_multiply_darkens_background() {
        let mut p = new_pixmap(80, 40);
        p.fill(tiny_skia::Color::from_rgba8(255, 255, 255, 255));
        let obj = Object {
            kind: Kind::Highlight {
                a: Point::new(10.0, 10.0),
                b: Point::new(70.0, 30.0),
                corner: 0.0,
            },
            style: Style::new([255, 240, 0, 255], 2.0),
        };
        draw_object(&mut p, &obj);

        // 白底（255）× 半透明黄：R 不变、G 略降、B 明显降 → 淡黄；黑色文字保持黑
        let (r, g, b, _) = pixel(&p, 40, 20);
        assert_eq!(r, 255, "白底 R 通道应保持（源 R=255）：r={r}");
        assert!(g < 255 && g > 200, "白底 G 通道应轻微下降（染黄）：g={g}");
        assert!(b > 40 && b < 160, "白底 B 通道应明显下降：b={b}");
        let (or, og, ob, _) = pixel(&p, 5, 5);
        assert_eq!((or, og, ob), (255, 255, 255), "区域外应保持纯白");
    }

    #[test]
    fn highlight_preserves_dark_text() {
        // 黑字（0）在高亮区内应保持黑色，不被色块覆盖
        let mut p = new_pixmap(40, 40);
        p.fill(tiny_skia::Color::from_rgba8(255, 255, 255, 255));
        let mut black = Paint::default();
        black.set_color_rgba8(0, 0, 0, 255);
        p.fill_rect(
            Rect::from_xywh(15.0, 19.0, 10.0, 3.0).unwrap(),
            &black,
            Transform::identity(),
            None,
        );
        let obj = Object {
            kind: Kind::Highlight {
                a: Point::new(10.0, 10.0),
                b: Point::new(30.0, 30.0),
                corner: 0.0,
            },
            style: Style::new([255, 240, 0, 255], 2.0),
        };
        draw_object(&mut p, &obj);

        let (r, _, _, _) = pixel(&p, 20, 20);
        assert_eq!(r, 0, "高亮区内黑色文字应保持黑色：r={r}");
    }

    #[test]
    fn blur_smooths_and_preserves_energy() {
        let mut p = new_pixmap(40, 40);
        p.fill(tiny_skia::Color::from_rgba8(0, 0, 0, 255));
        // 5×5 白色块，中心 (20..25)²
        for dy in 20..25 {
            for dx in 20..25 {
                let off = (dy * 40 + dx) * 4;
                p.data_mut()[off..off + 4].copy_from_slice(&[255, 255, 255, 255]);
            }
        }
        let before: u32 = p
            .data()
            .iter()
            .enumerate()
            .filter(|(i, _)| i % 4 == 0)
            .map(|(_, v)| *v as u32)
            .sum();
        let obj = Object {
            kind: Kind::Blur {
                a: Point::new(10.0, 10.0),
                b: Point::new(30.0, 30.0),
                radius: 2.0,
                corner: 0.0,
            },
            style: Style::new([0, 0, 0, 255], 2.0),
        };
        draw_object(&mut p, &obj);

        let center = pixel(&p, 22, 22).0;
        assert!(center > 20 && center < 255, "亮块中心应被平滑但保留亮度：r={center}");
        let spread = pixel(&p, 18, 22).0;
        assert!(spread > 0, "亮度应扩散到块外：r={spread}");
        assert_eq!(pixel(&p, 5, 5).0, 0, "模糊区域外应保持黑色");
        let after: u32 = p
            .data()
            .iter()
            .enumerate()
            .filter(|(i, _)| i % 4 == 0)
            .map(|(_, v)| *v as u32)
            .sum();
        assert!(
            (before as i64 - after as i64).abs() < before as i64 / 50,
            "模糊不应增删亮度：before={before} after={after}"
        );
    }

    #[test]
    fn blur_bake_respects_crop_translate() {
        // 覆盖层坐标：亮块 (108,108)-(113,113)，模糊对象 (105,105)-(115,115)；裁剪原点 (100,100)
        // → 模糊区落在裁剪图 (5,5)-(15,15)，亮块在 (8,8)-(13,13) 被模糊
        let mut p = new_pixmap(40, 40);
        p.fill(tiny_skia::Color::from_rgba8(0, 0, 0, 255));
        for dy in 8..13 {
            for dx in 8..13 {
                let off = (dy * 40 + dx) * 4;
                p.data_mut()[off..off + 4].copy_from_slice(&[255, 255, 255, 255]);
            }
        }
        let obj = Object {
            kind: Kind::Blur {
                a: Point::new(105.0, 105.0),
                b: Point::new(115.0, 115.0),
                radius: 2.0,
                corner: 0.0,
            },
            style: Style::new([0, 0, 0, 255], 2.0),
        };
        bake_objects(&mut p, &[obj], (100, 100));

        let (r, _, _, _) = pixel(&p, 10, 10);
        assert!(r > 20 && r < 255, "亮块中心应被模糊：r={r}");
        let (out, _, _, _) = pixel(&p, 2, 10);
        assert_eq!(out, 0, "模糊区外应保持黑色");
    }

    /// 人工核对用：`cargo test gradient_visual_dump -- --ignored --nocapture` 会落两张 PNG
    /// 到临时目录（填充矩形 / 描边矩形），看三段颜色在深底与细描边上是否都能辨。
    #[test]
    #[ignore]
    fn gradient_visual_dump() {
        let g = crate::editor::Gradient::seed([229, 57, 53, 255]);
        for (name, filled) in [("filled", true), ("stroke", false)] {
            let style = Style::new(g.representative(), 6.0).with_gradient(g);
            let obj = Object {
                kind: Kind::Rect {
                    a: Point::new(10.0, 10.0),
                    b: Point::new(390.0, 190.0),
                    radius: 0.0,
                    filled,
                },
                style,
            };
            let mut pm = new_pixmap(400, 200);
            pm.fill(tiny_skia::Color::from_rgba8(30, 30, 30, 255));
            draw_object(&mut pm, &obj);
            let path = std::env::temp_dir().join(format!("airsnip-gradient-{name}.png"));
            std::fs::write(&path, pm.encode_png().expect("png")).expect("写入 dump");
            eprintln!("已输出 {path:?}");
        }
        // 连续 6 套（链式推进）：亮度应恒定，不能一套套变暗
        let mut pm = new_pixmap(400, 300);
        pm.fill(tiny_skia::Color::from_rgba8(30, 30, 30, 255));
        let base = [229, 57, 53, 255];
        for i in 0..6u32 {
            let g = crate::editor::Gradient::nth(base, i);
            let style = Style::new(g.representative(), 4.0).with_gradient(g);
            let obj = Object {
                kind: Kind::Rect {
                    a: Point::new(10.0, 10.0 + i as f32 * 46.0),
                    b: Point::new(390.0, 46.0 + i as f32 * 46.0),
                    radius: 0.0,
                    filled: true,
                },
                style,
            };
            draw_object(&mut pm, &obj);
        }
        let path = std::env::temp_dir().join("airsnip-gradient-chain.png");
        std::fs::write(&path, pm.encode_png().expect("png")).expect("写入 dump");
        eprintln!("已输出 {path:?}");
        // 手绘（按弧长重采样后仍需连续、沿笔迹走色）
        let mut pm = new_pixmap(400, 220);
        pm.fill(tiny_skia::Color::from_rgba8(30, 30, 30, 255));
        let g = crate::editor::Gradient::seed([229, 57, 53, 255]);
        let pts: Vec<Point> = (0..=180)
            .map(|i| {
                let x = 10.0 + i as f32 * 2.0;
                Point::new(x, 110.0 + (i as f32 * 0.12).sin() * 70.0)
            })
            .collect();
        let obj = Object {
            kind: Kind::Freehand {
                pts: std::sync::Arc::new(pts),
            },
            style: Style::new(g.representative(), 6.0).with_gradient(g),
        };
        draw_object(&mut pm, &obj);
        let path = std::env::temp_dir().join("airsnip-gradient-freehand.png");
        std::fs::write(&path, pm.encode_png().expect("png")).expect("写入 dump");
        eprintln!("已输出 {path:?}");
    }
}
