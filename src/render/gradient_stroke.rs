// SPDX-License-Identifier: GPL-3.0-only
//! 沿路径的渐变描边（EDT-19）：矩形 / 椭圆 / 手绘的描边沿**周长弧长**走色，
//! 而不是沿某个直线方向——否则水平渐变下，矩形的上边与下边在同一 x 处颜色完全相同。
//!
//! 做法：把路径（含虚线切分）平坦化成折线 → 按整条路径的弧长重采样成 `STEP` 长的小段 →
//! 每小段用一段起点/终点取色的线性渐变描边（Round 端帽让相邻段重叠，避免细缝）。
//! 重采样看**累计弧长**，不是每条细碎折线各描一次——否则贝塞尔细分出的零点几像素的点
//! 会让一条笔迹产生上千次 `stroke_path`。
//! `LinearGradient` 本身只能沿直线走色，这是 tiny-skia 下的通用近似。

use tiny_skia::{LineCap, LineJoin, Path, PathBuilder, PathSegment, Pixmap, Stroke, StrokeDash, Transform};

use crate::editor::{Point, Style};

/// 每小段的目标长度（物理像素）：短到看不出台阶，长到不会产生上万次描边。
const STEP: f32 = 8.0;
/// 曲线平坦化的细分段数（一条三次贝塞尔切成多少折线）。
const SUBDIV: usize = 20;

/// 沿路径弧长渐变描边。调用方已确认 `style.gradient` 存在。
pub(super) fn stroke_gradient(pixmap: &mut Pixmap, path: &Path, style: &Style, tr: Transform) {
    let Some(g) = style.gradient.as_ref() else {
        return;
    };
    // 虚线先切分，切完的每段仍按整条路径的弧长取色
    let dashed;
    let path = match style.line.dash_array(style.width).and_then(|a| StrokeDash::new(a, 0.0)) {
        Some(dash) => match path.dash(&dash, 1.0) {
            Some(p) => {
                dashed = p;
                &dashed
            }
            None => path,
        },
        None => path,
    };
    let polylines = flatten(path);
    let total: f32 = polylines.iter().map(|l| polyline_len(l)).sum();
    if total < 0.5 {
        return;
    }
    // 先预采样一张颜色表（带缓存）：段内只查表插值，不再每段做 OKLab 转换（掉帧主因）
    let table = super::paint::sampled_cached(g, 112);
    // 虚线 / 点划线：每段独立且要平头（Round 会让虚线看成圆点），
    // 且虚线已被 `path.dash` 切碎，不需要额外重叠。
    let cap = if style.line.is_solid() {
        LineCap::Round
    } else {
        LineCap::Butt
    };
    let stroke = Stroke {
        width: style.width,
        line_cap: cap,
        line_join: LineJoin::Round,
        ..Default::default()
    };
    // 按**整条路径的弧长**重采样：每走满 `STEP` 才描一小段（局部渐变）。
    resample(&polylines, total, |a, b, t0, t1| {
        if let Some(path) = line_path(a, b)
            && let Some(paint) = local_paint(&table, a, b, t0, t1)
        {
            pixmap.stroke_path(&path, &paint, &stroke, tr, None);
        }
    });
}

/// 把平坦化折线按**整条路径的弧长**重采样成 `STEP` 长的小段，回调（起点, 终点, t0, t1）。
///
/// 不能按折线逐段 `ceil(len / STEP)`（至少 1 段）：曲线被细分成几十个零点几像素的点，
/// 每段都要 `stroke_path` 一次，一条长笔迹就上千次描边（手绘第二条起卡顿的根因，EDT-19）。
fn resample(polylines: &[Vec<Point>], total: f32, mut emit: impl FnMut(Point, Point, f32, f32)) {
    let mut acc = 0.0_f32; // 当前点累计弧长
    for line in polylines {
        let mut prev = line[0];
        let mut seg_start = prev;
        let mut seg_arc = acc;
        for &p in &line[1..] {
            let (dx, dy) = (p.x - prev.x, p.y - prev.y);
            let len = (dx * dx + dy * dy).sqrt();
            prev = p;
            if len < 1e-4 {
                continue;
            }
            acc += len;
            if acc - seg_arc < STEP {
                continue;
            }
            emit(seg_start, p, seg_arc / total, acc / total);
            seg_start = p;
            seg_arc = acc;
        }
        // 子路径收尾：不足 `STEP` 的尾巴也要画（虚线被切碎后每段本来就短）
        if acc - seg_arc > 1e-4 {
            emit(seg_start, prev, seg_arc / total, acc / total);
        }
    }
}

/// 直角矩形描边：**4 条边各一段线性渐变**（每边一次 `stroke_path`）。
/// 沿周长切段对直角矩形是浪费：上白段近百次描边，而直边上的线性渐变本来就是精确的。
/// 4 次 vs ~90 次，多对象拖动时的帧耗时差一个量级。
pub(super) fn stroke_rect_edges(pixmap: &mut Pixmap, rect: (f32, f32, f32, f32), style: &Style, tr: Transform) {
    let (x0, y0, x1, y1) = rect;
    let Some(g) = style.gradient.as_ref() else {
        return;
    };
    let (w, h) = (x1 - x0, y1 - y0);
    let per = 2.0 * (w + h);
    if per < 1.0 {
        return;
    }
    let table = super::paint::sampled_cached(g, 112);
    // 方头端点：两端各外扩半个线宽，正好把四个角补成直角（Butt 会在角上留缺口）。
    let stroke = Stroke {
        width: style.width,
        line_cap: LineCap::Square,
        line_join: LineJoin::Miter,
        ..Default::default()
    };
    let segs = [
        (Point::new(x0, y0), Point::new(x1, y0), 0.0, w / per),
        (Point::new(x1, y0), Point::new(x1, y1), w / per, (w + h) / per),
        (
            Point::new(x1, y1),
            Point::new(x0, y1),
            (w + h) / per,
            (2.0 * w + h) / per,
        ),
        (Point::new(x0, y1), Point::new(x0, y0), (2.0 * w + h) / per, 1.0),
    ];
    for (a, b, t0, t1) in segs {
        if let (Some(path), Some(paint)) = (line_path(a, b), local_paint(&table, a, b, t0, t1)) {
            pixmap.stroke_path(&path, &paint, &stroke, tr, None);
        }
    }
}

/// 小段路径：两端各外扩半个线宽（由 Round 端帽覆盖），段间不留缝。
fn line_path(a: Point, b: Point) -> Option<Path> {
    let mut pb = PathBuilder::new();
    pb.move_to(a.x, a.y);
    pb.line_to(b.x, b.y);
    pb.finish()
}

fn local_paint(table: &[(f32, [u8; 4])], a: Point, b: Point, t0: f32, t1: f32) -> Option<tiny_skia::Paint<'static>> {
    let c0 = super::paint::table_color(table, t0);
    let c1 = super::paint::table_color(table, t1);
    let stops = vec![
        tiny_skia::GradientStop::new(0.0, tiny_skia::Color::from_rgba8(c0[0], c0[1], c0[2], c0[3])),
        tiny_skia::GradientStop::new(1.0, tiny_skia::Color::from_rgba8(c1[0], c1[1], c1[2], c1[3])),
    ];
    let shader = tiny_skia::LinearGradient::new(
        tiny_skia::Point { x: a.x, y: a.y },
        tiny_skia::Point { x: b.x, y: b.y },
        stops,
        tiny_skia::SpreadMode::Pad,
        Transform::identity(),
    )?;
    Some(tiny_skia::Paint {
        shader,
        ..Default::default()
    })
}

fn polyline_len(l: &[Point]) -> f32 {
    l.windows(2)
        .map(|w| {
            let (dx, dy) = (w[1].x - w[0].x, w[1].y - w[0].y);
            (dx * dx + dy * dy).sqrt()
        })
        .sum()
}

/// 把路径平坦化成折线（每条子路径一条）。
fn flatten(path: &Path) -> Vec<Vec<Point>> {
    let mut out = Vec::new();
    let mut cur: Vec<Point> = Vec::new();
    let p = |p: tiny_skia::Point| Point::new(p.x, p.y);
    for seg in path.segments() {
        match seg {
            PathSegment::MoveTo(pt) => {
                if cur.len() > 1 {
                    out.push(std::mem::take(&mut cur));
                }
                cur.push(p(pt));
            }
            PathSegment::LineTo(pt) => cur.push(p(pt)),
            PathSegment::QuadTo(c, e) => {
                let Some(start) = cur.last().copied() else { continue };
                for i in 1..=SUBDIV {
                    let t = i as f32 / SUBDIV as f32;
                    let u = 1.0 - t;
                    cur.push(Point::new(
                        u * u * start.x + 2.0 * u * t * c.x + t * t * e.x,
                        u * u * start.y + 2.0 * u * t * c.y + t * t * e.y,
                    ));
                }
            }
            PathSegment::CubicTo(c1, c2, e) => {
                let Some(s) = cur.last().copied() else { continue };
                for i in 1..=SUBDIV {
                    let t = i as f32 / SUBDIV as f32;
                    let u = 1.0 - t;
                    let (u2, t2, ut) = (u * u * u, t * t * t, 3.0 * u * u * t);
                    let (u3, t3) = (3.0 * u * t * t, 0.0);
                    let _ = t3;
                    cur.push(Point::new(
                        u2 * s.x + ut * c1.x + u3 * c2.x + t2 * e.x,
                        u2 * s.y + ut * c1.y + u3 * c2.y + t2 * e.y,
                    ));
                }
            }
            PathSegment::Close => {
                if let Some(first) = cur.first().copied() {
                    cur.push(first);
                }
                if cur.len() > 1 {
                    out.push(std::mem::take(&mut cur));
                }
            }
        }
    }
    if cur.len() > 1 {
        out.push(cur);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::editor::Gradient;
    use crate::editor::line_store;
    use crate::editor::{Kind, Object};
    use crate::render::draw_object;

    fn canvas(w: u32, h: u32) -> Pixmap {
        let mut p = Pixmap::new(w, h).unwrap();
        p.fill(tiny_skia::Color::from_rgba8(30, 30, 30, 255));
        p
    }

    /// 渐变 + 虚线 + 直角矩形：必须仍是虚线。
    /// （直角矩形走 4 段快速路径，它用 Square 端帽填角；若把虚线也塞进那条路径，
    /// 间隔会被填成实线——这里守门。）
    #[test]
    fn dashed_gradient_rect_keeps_gaps() {
        let g = Gradient::seed([229, 57, 53, 255]);
        let style = Style::new(g.representative(), 6.0)
            .with_line(crate::editor::LineStyle::Dashed)
            .with_gradient(g);
        let obj = Object {
            kind: Kind::Rect {
                a: Point::new(10.0, 10.0),
                b: Point::new(190.0, 90.0),
                radius: 0.0,
                filled: false,
            },
            style,
        };
        let mut pm = canvas(200, 100);
        draw_object(&mut pm, &obj);
        // 背景不透明（alpha 恒 255），要按「与背景色不同」判墨
        let bg = (30u8, 30u8, 30u8);
        let mut runs = 0;
        let mut prev = false;
        for x in 10..190 {
            let ink = pm.pixel(x, 11).is_some_and(|p| (p.red(), p.green(), p.blue()) != bg);
            if ink && !prev {
                runs += 1;
            }
            prev = ink;
        }
        assert!(runs >= 3, "渐变虚线矩形应仍有间隔，实际 {runs} 段");
    }

    /// tiny-skia 的 dash 在矩形路径上应能切出多段（诊断用）。
    #[test]
    fn tiny_skia_dash_works_on_rect_path() {
        let mut pb = PathBuilder::new();
        pb.push_rect(tiny_skia::Rect::from_ltrb(10.0, 10.0, 190.0, 90.0).unwrap());
        let p = pb.finish().expect("rect path");
        let d = StrokeDash::new(vec![18.0, 12.0], 0.0).expect("dash");
        let dashed = p.dash(&d, 1.0).expect("dash 应成功");
        let segs = dashed.segments().count();
        assert!(segs >= 4, "虚线应切成多段，实际 {segs}");
    }

    /// 矩形描边沿周长走色：上边与下边在同一 x 处必须不同色（水平渐变时它们是一样的）。
    #[test]
    fn rect_stroke_gradient_wraps_around() {
        let g = Gradient::seed([229, 57, 53, 255]);
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
        let mut pm = canvas(200, 100);
        draw_object(&mut pm, &obj);
        let at = |x: u32, y: u32| pm.pixel(x, y).map(|c| (c.red(), c.green(), c.blue()));
        let top = at(100, 11).expect("上边框");
        let bottom = at(100, 89).expect("下边框");
        assert_ne!(top, bottom, "上下边框同一列应不同色：{top:?} vs {bottom:?}");
        // 整圈都要有颜色变化：上边左 / 右也应不同
        let (tl, tr) = (at(20, 11).unwrap(), at(180, 11).unwrap());
        assert_ne!(tl, tr, "上边左右应不同色");
        let _ = line_store;
    }

    /// 细碎折线（每点只差 0.1px，与贝塞尔细分出来的点列一样）必须**按弧长合段**：
    /// 1000px 的笔迹只该切成 ~1000/STEP 段，而不是每个点描一次（那个写法让手绘第二条起卡顿）。
    #[test]
    fn resample_merges_fine_polyline() {
        let pts: Vec<Point> = (0..=10_000).map(|i| Point::new(i as f32 * 0.1, 0.0)).collect();
        let (mut n, mut prev_end, mut prev_t1) = (0usize, None, 0.0_f32);
        resample(&[pts], 1000.0, |a, b, t0, t1| {
            n += 1;
            if let Some(e) = prev_end {
                assert_eq!((a.x, a.y), e, "小段必须首尾相接，否则笔画会断开");
                assert!((t0 - prev_t1).abs() < 1e-5, "取色参数也要接上");
            }
            assert!(b.x - a.x <= STEP + 0.2, "小段不应超过 STEP：{}", b.x - a.x);
            prev_end = Some((b.x, b.y));
            prev_t1 = t1;
        });
        let expect = (1000.0 / STEP) as usize;
        assert!(n + 2 >= expect && n <= expect + 1, "应切成 ~{expect} 段，实际 {n} 段");
    }

    /// 手绘渐变描边不能有缝：按弧长重采样后相邻小段仍是首尾相接的。
    /// （曾按平坦化折线逐段 `ceil(len / STEP)`，每条细碎子段都描一次，长笔迹上千次 `stroke_path`，
    /// 第二条笔画起明显卡顿 —— 这里只守「重采样后仍然连续」，性能由少一个数量级的描边次数保证。）
    #[test]
    fn gradient_freehand_has_no_gaps() {
        let g = Gradient::seed([229, 57, 53, 255]);
        let style = Style::new(g.representative(), 6.0).with_gradient(g);
        let pts: Vec<Point> = (0..=90).map(|i| Point::new(20.0 + i as f32 * 4.0, 50.0)).collect();
        let obj = Object {
            kind: Kind::Freehand {
                pts: std::sync::Arc::new(pts),
            },
            style,
        };
        let mut pm = canvas(400, 100);
        draw_object(&mut pm, &obj);
        let bg = (30u8, 30u8, 30u8);
        for x in 24..376 {
            let ink = pm.pixel(x, 50).is_some_and(|p| (p.red(), p.green(), p.blue()) != bg);
            assert!(ink, "手绘渐变在第 {x} 列断开了");
        }
    }
}
