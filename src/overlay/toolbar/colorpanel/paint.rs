// SPDX-License-Identifier: GPL-3.0-only
//! 自定义颜色面板绘制（EDT-7，issue #73）：色相条（横向渐变）、饱和度 / 明度方块
//! （纯色 + 白色横向渐变 + 黑色纵向渐变叠加）、取样标记与快捷色块。纯 CPU 渲染。

use tiny_skia::{Color, GradientStop, LinearGradient, PathBuilder, Pixmap, Rect, SpreadMode, Stroke, Transform};

use crate::editor::palette::{QUICK, hsv_to_rgb};
use crate::overlay::SelRect;
use crate::overlay::shape::rounded_rect;
use crate::theme::Palette;

use super::ColorPanel;

/// 色相条渐变的 7 个锚点（红→黄→绿→青→蓝→品→红）。
const HUE_STOPS: [(u8, u8, u8); 7] = [
    (255, 0, 0),
    (255, 255, 0),
    (0, 255, 0),
    (0, 255, 255),
    (0, 0, 255),
    (255, 0, 255),
    (255, 0, 0),
];

/// 绘制整个取色面板。
pub(in crate::overlay) fn render(pixmap: &mut Pixmap, panel: &ColorPanel, p: &Palette) {
    let ui = panel.ui();
    rounded_rect(pixmap, panel.rect, ui.f(6.0), p.bg, Some(p.border));
    draw_hue_strip(pixmap, panel);
    draw_sv_square(pixmap, panel);
    draw_hue_marker(pixmap, panel);
    draw_sv_marker(pixmap, panel);
    draw_quick(pixmap, panel, p);
}

/// 色相条：横向渐变。
fn draw_hue_strip(pixmap: &mut Pixmap, panel: &ColorPanel) {
    let r = panel.hue_rect();
    let stops = HUE_STOPS
        .iter()
        .enumerate()
        .map(|(i, &(rr, gg, bb))| GradientStop::new(i as f32 / 6.0, Color::from_rgba8(rr, gg, bb, 255)))
        .collect();
    let (x0, y0) = (r.x as f32, r.y as f32);
    let (x1, y1) = ((r.x + r.w) as f32, (r.y + r.h) as f32);
    let Some(shader) = LinearGradient::new(
        tiny_skia::Point::from_xy(x0, y0),
        tiny_skia::Point::from_xy(x1, y0),
        stops,
        SpreadMode::Pad,
        Transform::identity(),
    ) else {
        return;
    };
    let mut paint = tiny_skia::Paint::default();
    paint.shader = shader;
    if let Some(rect) = Rect::from_ltrb(x0, y0, x1, y1) {
        pixmap.fill_rect(rect, &paint, Transform::identity(), None);
    }
}

/// 饱和度 / 明度方块：纯色底 + 白色横向渐变（饱和）+ 黑色纵向渐变（明度）。
fn draw_sv_square(pixmap: &mut Pixmap, panel: &ColorPanel) {
    let r = panel.sv_rect();
    let (x0, y0) = (r.x as f32, r.y as f32);
    let (x1, y1) = ((r.x + r.w) as f32, (r.y + r.h) as f32);
    let Some(rect) = Rect::from_ltrb(x0, y0, x1, y1) else {
        return;
    };
    let hue_rgb = hsv_to_rgb(panel.hsv().0, 1.0, 1.0);
    let mut base = tiny_skia::Paint::default();
    base.set_color_rgba8(hue_rgb[0], hue_rgb[1], hue_rgb[2], 255);
    pixmap.fill_rect(rect, &base, Transform::identity(), None);

    // 左侧白（饱和度 0）→ 右侧透明（饱和度 1）
    let white = vec![
        GradientStop::new(0.0, Color::from_rgba8(255, 255, 255, 255)),
        GradientStop::new(1.0, Color::from_rgba8(255, 255, 255, 0)),
    ];
    fill_gradient(pixmap, rect, x0, y0, x1, y0, white);
    // 上透明（明度 1）→ 下黑（明度 0）
    let black = vec![
        GradientStop::new(0.0, Color::from_rgba8(0, 0, 0, 0)),
        GradientStop::new(1.0, Color::from_rgba8(0, 0, 0, 255)),
    ];
    fill_gradient(pixmap, rect, x0, y0, x0, y1, black);
}

/// 用线性渐变填充矩形。
fn fill_gradient(pixmap: &mut Pixmap, rect: Rect, x0: f32, y0: f32, x1: f32, y1: f32, stops: Vec<GradientStop>) {
    let Some(shader) = LinearGradient::new(
        tiny_skia::Point::from_xy(x0, y0),
        tiny_skia::Point::from_xy(x1, y1),
        stops,
        SpreadMode::Pad,
        Transform::identity(),
    ) else {
        return;
    };
    let mut paint = tiny_skia::Paint::default();
    paint.shader = shader;
    pixmap.fill_rect(rect, &paint, Transform::identity(), None);
}

/// 色相条上的位置标记（竖条）。
fn draw_hue_marker(pixmap: &mut Pixmap, panel: &ColorPanel) {
    let r = panel.hue_rect();
    let x = r.x + (panel.hsv().0 / 360.0 * r.w as f32).round() as i32;
    let (w, inset) = (panel.ui().px(3.0), panel.ui().px(1.0));
    let bar = SelRect {
        x: (x - w / 2).clamp(r.x, r.x + r.w - w),
        y: r.y + inset,
        w,
        h: r.h - inset * 2,
    };
    rounded_rect(
        pixmap,
        bar,
        panel.ui().f(1.5),
        [255, 255, 255, 255],
        Some([0, 0, 0, 140]),
    );
}

/// 明度方块上的取样标记（圆环，白环 + 暗色描边保证深浅底都可见）。
fn draw_sv_marker(pixmap: &mut Pixmap, panel: &ColorPanel) {
    let r = panel.sv_rect();
    let (s, v) = (panel.hsv().1, panel.hsv().2);
    let cx = r.x as f32 + s * r.w as f32;
    let cy = r.y as f32 + (1.0 - v) * r.h as f32;
    let rad = panel.ui().f(5.0);
    ring(pixmap, cx, cy, rad, [0, 0, 0, 160], panel.ui().f(3.0));
    ring(pixmap, cx, cy, rad, [255, 255, 255, 255], panel.ui().f(2.0));
}

/// 描边圆环。
fn ring(pixmap: &mut Pixmap, cx: f32, cy: f32, r: f32, color: [u8; 4], width: f32) {
    let mut pb = PathBuilder::new();
    pb.push_circle(cx, cy, r);
    let Some(path) = pb.finish() else {
        return;
    };
    let mut paint = tiny_skia::Paint::default();
    paint.set_color_rgba8(color[0], color[1], color[2], color[3]);
    let stroke = Stroke {
        width,
        ..Default::default()
    };
    pixmap.stroke_path(&path, &paint, &stroke, Transform::identity(), None);
}

/// 快捷色块：命中悬停或与当前色相同时高亮。
fn draw_quick(pixmap: &mut Pixmap, panel: &ColorPanel, p: &Palette) {
    let current = panel.color();
    for (i, &c) in QUICK.iter().enumerate() {
        let r = panel.quick_rect(i);
        rounded_rect(pixmap, r, panel.ui().f(4.0), c, Some(p.swatch_border));
        let same = c[..3] == current[..3];
        if panel.hover() == Some(i) || same {
            crate::overlay::shape::rounded_rect_outline(pixmap, r, panel.ui().f(4.0), p.accent);
        }
    }
}
