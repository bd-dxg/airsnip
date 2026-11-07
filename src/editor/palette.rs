// SPDX-License-Identifier: GPL-3.0-only
//! 标注颜色调色板（EDT-7）：5 个预设色 + 1 个自定义色槽（issue #73）。
//! 自定义色由工具栏「自定义颜色」色块展开的自绘取色面板选取（见 `overlay::toolbar::colorpanel`），
//! 覆盖层线程写入 `custom_color_store`，主线程在截图结束时写回配置持久化。
//! 这里另提供 HSV↔RGB 与取色面板的快捷色常量 `QUICK`。

/// 预设颜色调色板（RGBA）。透明度由工具栏后续提供，当前固定 255。
pub const PALETTE: [[u8; 4]; 5] = [
    [255, 255, 255, 255],
    [229, 57, 53, 255],
    [251, 140, 0, 255],
    [67, 160, 71, 255],
    [30, 136, 229, 255],
];

/// 自定义色的色块下标（紧跟预设色之后）。
pub const CUSTOM_INDEX: usize = PALETTE.len();

/// 色块总数（预设 + 自定义）。
pub const SWATCH_COUNT: usize = PALETTE.len() + 1;

/// 自定义色默认值（Material 紫 600）：用户首次取色前工具栏展示的颜色。
pub const CUSTOM_DEFAULT: [u8; 4] = [142, 36, 170, 255];

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

/// 取第 `index` 个色块的颜色；`index` 为自定义槽（或越界）时返回 `custom`。
pub fn swatch(index: usize, custom: [u8; 4]) -> [u8; 4] {
    PALETTE.get(index).copied().unwrap_or(custom)
}

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

/// 覆盖层线程选定的自定义色：主线程在截图结束时取出并写回配置（同 `output::recent`）。
static CUSTOM_COLOR: std::sync::Mutex<Option<[u8; 4]>> = std::sync::Mutex::new(None);

/// 记录本次选定的自定义色（覆盖层线程调用）。
pub fn custom_color_store(c: [u8; 4]) {
    if let Ok(mut g) = CUSTOM_COLOR.lock() {
        *g = Some(c);
    }
}

/// 取出并清空记录的自定义色（主线程写回配置用）。
pub fn custom_color_take() -> Option<[u8; 4]> {
    CUSTOM_COLOR.lock().ok().and_then(|mut g| g.take())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 色块取值：预设返回预设，自定义槽与越界返回自定义色。
    #[test]
    fn swatch_maps_index() {
        let custom = [1, 2, 3, 4];
        for i in 0..PALETTE.len() {
            assert_eq!(swatch(i, custom), PALETTE[i]);
        }
        assert_eq!(swatch(CUSTOM_INDEX, custom), custom);
        assert_eq!(swatch(CUSTOM_INDEX + 5, custom), custom, "越界回退自定义色");
    }

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
    fn custom_color_memory_roundtrip() {
        assert_eq!(custom_color_take(), None, "初始应为空");
        custom_color_store([10, 20, 30, 255]);
        assert_eq!(custom_color_take(), Some([10, 20, 30, 255]));
        assert_eq!(custom_color_take(), None, "取出后应清空");
    }
}
