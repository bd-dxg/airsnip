// SPDX-License-Identifier: GPL-3.0-only
//! 标注颜色调色板（EDT-7）：5 个可编辑色块（issue #73 后续）。
//! 单击色块即使用；双击展开自绘取色面板（见 `overlay::toolbar::colorpanel`）改写该色块。
//! 覆盖层线程写入 `colors_store`，主线程在截图结束时写回配置持久化。
//! 这里另提供 HSV↔RGB 与取色面板的快捷色常量 `QUICK`。

/// 色块数量。
pub const SWATCH_COUNT: usize = 5;

/// 色块默认颜色（RGBA）：用户未改过时使用。透明度当前固定 255。
pub const DEFAULT_COLORS: [[u8; 4]; SWATCH_COUNT] = [
    [255, 255, 255, 255],
    [229, 57, 53, 255],
    [251, 140, 0, 255],
    [67, 160, 71, 255],
    [30, 136, 229, 255],
];

/// 取色面板的快捷色块（2 行 × 6 列）：常用 Material 色 + 黑 / 白 / 灰。
pub const QUICK: [[u8; 4]; 12] = [
    [255, 255, 255, 255],
    [0, 0, 0, 255],
    [158, 158, 158, 255],
    [229, 57, 53, 255],
    [251, 140, 0, 255],
    [253, 216, 53, 255],
    [67, 160, 71, 255],
    [0, 172, 193, 255],
    [30, 136, 229, 255],
    [142, 36, 170, 255],
    [216, 27, 96, 255],
    [109, 76, 65, 255],
];

/// HSV → RGB：`h` 为色相角度（0..360，可取 360），`s`/`v` 为 0..1；输出 alpha 固定 255。
pub fn hsv_to_rgb(h: f32, s: f32, v: f32) -> [u8; 4] {
    let h = h.rem_euclid(360.0) / 60.0;
    let s = s.clamp(0.0, 1.0);
    let v = v.clamp(0.0, 1.0);
    let c = v * s;
    let x = c * (1.0 - (h % 2.0 - 1.0).abs());
    let (r, g, b) = match h as i32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let m = v - c;
    let to = |f: f32| ((f + m) * 255.0).round() as u8;
    [to(r), to(g), to(b), 255]
}

/// RGB → HSV：返回 `(h 0..360, s 0..1, v 0..1)`。
pub fn rgb_to_hsv(c: [u8; 4]) -> (f32, f32, f32) {
    let (r, g, b) = (c[0] as f32 / 255.0, c[1] as f32 / 255.0, c[2] as f32 / 255.0);
    let (max, min) = (r.max(g).max(b), r.min(g).min(b));
    let d = max - min;
    let h = if d < 1e-6 {
        0.0
    } else if max == r {
        60.0 * ((g - b) / d % 6.0)
    } else if max == g {
        60.0 * ((b - r) / d + 2.0)
    } else {
        60.0 * ((r - g) / d + 4.0)
    };
    let h = if h < 0.0 { h + 360.0 } else { h };
    let s = if max < 1e-6 { 0.0 } else { d / max };
    (h, s, max)
}

/// 覆盖层线程改过的色块组：主线程在截图结束时取出并写回配置（同 `output::recent`）。
static COLORS: std::sync::Mutex<Option<[[u8; 4]; SWATCH_COUNT]>> = std::sync::Mutex::new(None);

/// 记录本次改过的色块组（覆盖层线程调用）。
pub fn colors_store(c: [[u8; 4]; SWATCH_COUNT]) {
    if let Ok(mut g) = COLORS.lock() {
        *g = Some(c);
    }
}

/// 取出并清空记录的色块组（主线程写回配置用）。
pub fn colors_take() -> Option<[[u8; 4]; SWATCH_COUNT]> {
    COLORS.lock().ok().and_then(|mut g| g.take())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// HSV 往返：纯色与灰色的 RGB→HSV→RGB 能还原（量化误差 ≤1）。
    #[test]
    fn hsv_roundtrip() {
        for c in [
            [229, 57, 53, 255],
            [30, 136, 229, 255],
            [67, 160, 71, 255],
            [0, 0, 0, 255],
        ] {
            let (h, s, v) = rgb_to_hsv(c);
            let back = hsv_to_rgb(h, s, v);
            for k in 0..3 {
                assert!((back[k] as i32 - c[k] as i32).abs() <= 1, "{c:?} → {back:?}");
            }
        }
        // 灰色无色相，饱和度应为 0
        let (_, s, v) = rgb_to_hsv([128, 128, 128, 255]);
        assert!(s.abs() < 1e-6);
        assert!((v - 128.0 / 255.0).abs() < 1e-6);
    }

    /// HSV 基准：红 / 绿 / 蓝 / 白 / 黑。
    #[test]
    fn hsv_primary_colors() {
        assert_eq!(hsv_to_rgb(0.0, 1.0, 1.0), [255, 0, 0, 255]);
        assert_eq!(hsv_to_rgb(120.0, 1.0, 1.0), [0, 255, 0, 255]);
        assert_eq!(hsv_to_rgb(240.0, 1.0, 1.0), [0, 0, 255, 255]);
        assert_eq!(hsv_to_rgb(0.0, 0.0, 1.0), [255, 255, 255, 255]);
        assert_eq!(hsv_to_rgb(0.0, 1.0, 0.0), [0, 0, 0, 255]);
    }

    /// 运行时记忆：写入后能取出，取出即清空。
    #[test]
    fn colors_memory_roundtrip() {
        assert_eq!(colors_take(), None, "初始应为空");
        let mut c = DEFAULT_COLORS;
        c[2] = [10, 20, 30, 255];
        colors_store(c);
        assert_eq!(colors_take(), Some(c));
        assert_eq!(colors_take(), None, "取出后应清空");
    }
}
