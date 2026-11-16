// SPDX-License-Identifier: GPL-3.0-only
//! 涂色（EDT-19）：纯色与三色标线性渐变。
//! 从 `render::mod` 拆出，控制单文件规模；所有绘制入口都从这里取 `Paint`。

use std::sync::{Arc, Mutex};

use tiny_skia::{Color, GradientStop, LinearGradient, Paint, Shader, SpreadMode, Transform};

use crate::editor::{Gradient, Point, Style};

/// 预采样表（位置, 颜色）。
pub(crate) type ColorTable = Arc<Vec<(f32, [u8; 4])>>;

/// 预采样表缓存（小容量 LRU，key = 渐变 + 段数）。
/// 同帧会渲染多个**不同**渐变的对象，单槽缓存会互相踢掉、每个对象都重算（掉帧源）；
/// 段数（48 / 112）也参与 key，否则两种精度会挤同一个槽。
static SAMPLE_CACHE: Mutex<Vec<(Gradient, usize, ColorTable)>> = Mutex::new(Vec::new());
/// 最多缓存几套渐变（远近于同屏对象数，内存可忽略）。
const CACHE_SLOTS: usize = 16;

/// 取缓存的颜色表；未命中才重算，命中即移到队尾（LRU）。
pub(crate) fn sampled_cached(g: &Gradient, n: usize) -> ColorTable {
    let mut cache = match SAMPLE_CACHE.lock() {
        Ok(c) => c,
        Err(e) => e.into_inner(),
    };
    if let Some(pos) = cache.iter().position(|(c, k, _)| c == g && *k == n) {
        let hit = cache.remove(pos);
        let table = hit.2.clone();
        cache.push(hit);
        return table;
    }
    let table = Arc::new(g.sampled(n));
    cache.push((*g, n, table.clone()));
    if cache.len() > CACHE_SLOTS {
        cache.remove(0);
    }
    table
}

/// 查表取色（表已密，段内直接 sRGB 插值，误差不可见）：避免每小段再做 OKLab。
pub(crate) fn table_color(table: &[(f32, [u8; 4])], t: f32) -> [u8; 4] {
    if table.len() < 2 {
        return table.first().map(|s| s.1).unwrap_or([0, 0, 0, 255]);
    }
    let x = t.clamp(0.0, 1.0) * (table.len() - 1) as f32;
    let i = (x.floor() as usize).min(table.len() - 2);
    let f = x - i as f32;
    let (a, b) = (table[i].1, table[i + 1].1);
    let mix = |k: usize| (a[k] as f32 + (b[k] as f32 - a[k] as f32) * f).round() as u8;
    [mix(0), mix(1), mix(2), mix(3)]
}

/// 纯色画刷（不涉及渐变）：图标、箭头、色块这些「非对象」绘制也用它。
pub(crate) fn solid_paint(color: [u8; 4]) -> Paint<'static> {
    let mut paint = Paint::default();
    paint.set_color_rgba8(color[0], color[1], color[2], color[3]);
    paint
}

/// 构造涂色：纯色走原路；带渐变时构造多色标 `LinearGradient`（EDT-19）。
/// `a`→`b` 是**对象坐标**下的渐变方向（箭头 / 直线沿起点→终点，其余按包围盒长边）。
///
/// **shader 不带变换**：`tiny-skia` 的 `fill_path` / `stroke_path` 会自己把绘制变换同时应用到
/// 路径与 shader 上。这里再叠一份会在导出烘焙（非 identity 平移）时把渐变平移两次、落出画布，
/// `Pad` 模式取端点色——屏幕上看着正常，导出就变成纯色。
pub(crate) fn paint_of(style: &Style, a: Point, b: Point) -> Paint<'static> {
    if let Some(shader) = style.gradient.as_ref().and_then(|g| gradient_shader(g, a, b)) {
        return Paint {
            shader,
            ..Default::default()
        };
    }
    solid_paint(style.color)
}

/// 渐变方向取包围盒的**长边**：宽 ≥ 高 → 左→右，否则 上→下。
/// 矩形 / 椭圆 / 手绘 / 序号徽标都走它：细描边（2–4px）让渐变沿长边完整走一遍，
/// 三段颜色才看得见（对角线会把三色摊到四条边上，看起来只像两色，ADR 0043）。
pub(crate) fn box_span(x0: f32, y0: f32, x1: f32, y1: f32) -> (Point, Point) {
    let (cx, cy) = ((x0 + x1) * 0.5, (y0 + y1) * 0.5);
    if (x1 - x0) >= (y1 - y0) {
        (Point::new(x0, cy), Point::new(x1, cy))
    } else {
        (Point::new(cx, y0), Point::new(cx, y1))
    }
}

/// 三色标渐变着色器：先把色标在 **OKLab** 里重采样成几十段（`Gradient::sampled`），
/// 再交给 tiny-skia——它只会在相邻 stop 间做 sRGB 线性插值，段足够短时误差不可见，
/// 也就不会在段内穿出灰区（EDT-19）。
/// 起止点重合 / 色标非法时返回 `None`（调用方退回纯色）。
fn gradient_shader(g: &Gradient, a: Point, b: Point) -> Option<Shader<'static>> {
    let stops: Vec<GradientStop> = sampled_cached(g, 48)
        .iter()
        .map(|(t, c)| GradientStop::new(*t, Color::from_rgba8(c[0], c[1], c[2], c[3])))
        .collect();
    let sa = tiny_skia::Point { x: a.x, y: a.y };
    let sb = tiny_skia::Point { x: b.x, y: b.y };
    LinearGradient::new(sa, sb, stops, SpreadMode::Pad, Transform::identity())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 多套不同渐变同帧渲染：缓存要分别命中，不能互相踢掉（否则每个对象都重算 112 段）。
    #[test]
    fn cache_keeps_multiple_gradients() {
        let base = [229, 57, 53, 255];
        let a = Gradient::nth(base, 0);
        let b = Gradient::nth(base, 1);
        let ta = sampled_cached(&a, 48);
        let tb = sampled_cached(&b, 48);
        assert!(!Arc::ptr_eq(&ta, &tb), "不同渐变不应共用同一张表");
        let ta_again = sampled_cached(&a, 48);
        assert!(Arc::ptr_eq(&ta, &ta_again), "同一渐变应命中缓存，不被 b 踢掉");
        // 段数也是 key：48 与 112 不能共槽
        let ta112 = sampled_cached(&a, 112);
        assert!(!Arc::ptr_eq(&ta, &ta112));
    }
}
