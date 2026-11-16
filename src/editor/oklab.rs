// SPDX-License-Identifier: GPL-3.0-only
//! OKLab 颜色空间（EDT-19）：渐变插值必须在感知空间里做。
//!
//! **为什么不能在 sRGB 里插值**：sRGB 是 gamma 编码的显示值，两色直接线性插值时，
//! 中间会穿过「两个通道同时掉」的区域，表现为发灰、发浑——也就是脏区。
//! OKLab 的 L 是感知亮度，插值路径不穿灰心。
//!
//! 这里实现：sRGB ⇄ 线性 RGB ⇄ OKLab/OKLCH、OKLab 插值、越界时保持 L/h 二分降 C 的色域映射、
//! 以及沿渐变参数的一维有序（Bayer 4×4）抖动，用来压色带。
//! 矩阵与 gamma 曲线固定为标准值（Björn Ottosson 的 OKLab），不要自行改系数。
// 标准系数的完整位数是规格的一部分（不得自行改），f32 精度内的多余位由 clippy 提示属预期。
#![allow(clippy::excessive_precision)]

/// OKLab 三元组：L ∈ 约 0..1，a/b 约 -0.4..0.4。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Lab {
    pub l: f32,
    pub a: f32,
    pub b: f32,
}

/// OKLCH 三元组：`h` 为角度。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Lch {
    pub l: f32,
    pub c: f32,
    pub h: f32,
}

// ---------------------------------------------------------------- gamma

fn srgb_to_linear(c: f32) -> f32 {
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

fn linear_to_srgb(c: f32) -> f32 {
    if c <= 0.003_130_8 {
        c * 12.92
    } else {
        1.055 * c.powf(1.0 / 2.4) - 0.055
    }
}

// ---------------------------------------------------------------- sRGB ⇄ OKLab

/// sRGB（u8）→ OKLab。alpha 不参与颜色转换。
pub fn srgb_to_oklab(c: [u8; 4]) -> Lab {
    let [r, g, b] = [c[0], c[1], c[2]].map(|v| srgb_to_linear(v as f32 / 255.0));
    let l = 0.412_221_470_8 * r + 0.536_332_536_3 * g + 0.051_445_992_9 * b;
    let m = 0.211_903_498_2 * r + 0.680_699_545_1 * g + 0.107_396_956_6 * b;
    let s = 0.088_302_461_9 * r + 0.281_718_837_6 * g + 0.629_978_700_5 * b;
    let (l_, m_, s_) = (l.cbrt(), m.cbrt(), s.cbrt());
    Lab {
        l: 0.210_454_255_3 * l_ + 0.793_617_785_0 * m_ - 0.004_072_046_8 * s_,
        a: 1.977_998_495_1 * l_ - 2.428_592_205_0 * m_ + 0.450_593_709_9 * s_,
        b: 0.025_904_037_1 * l_ + 0.782_771_766_2 * m_ - 0.808_675_766_0 * s_,
    }
}

/// OKLab → 线性 RGB（可能越界，不 clamp）。
fn oklab_to_linear(lab: Lab) -> [f32; 3] {
    let l_ = lab.l + 0.396_337_777_4 * lab.a + 0.215_803_757_3 * lab.b;
    let m_ = lab.l - 0.105_561_345_8 * lab.a - 0.063_854_172_8 * lab.b;
    let s_ = lab.l - 0.089_484_177_5 * lab.a - 1.291_485_548_0 * lab.b;
    let (l, m, s) = (l_ * l_ * l_, m_ * m_ * m_, s_ * s_ * s_);
    [
        4.076_741_662_1 * l - 3.307_711_591_3 * m + 0.230_969_929_2 * s,
        -1.268_438_004_6 * l + 2.609_757_401_1 * m - 0.341_319_396_5 * s,
        -0.004_196_086_3 * l - 0.703_418_614_7 * m + 1.707_614_701_0 * s,
    ]
}

fn in_gamut(lin: [f32; 3]) -> bool {
    lin.iter().all(|v| (-1e-4..=1.0 + 1e-4).contains(v))
}

/// 在给定亮度 `l` 与色相 `h` 下，sRGB 色域内能取到的**最大色度**（二分，18 次收敛到亚像素）。
/// 用来生成「每个色相都尽量鲜亮」的彩虹：同一 L 下不同色相的最大 C 差很多，
/// 固定 C 会让黄绿发浑、红蓝偏淡。
pub fn peak_chroma(l: f32, h: f32) -> f32 {
    let (mut lo, mut hi) = (0.0_f32, 0.5_f32);
    for _ in 0..18 {
        let mid = (lo + hi) * 0.5;
        if in_gamut(oklab_to_linear(lch_to_lab(Lch { l, c: mid, h }))) {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    lo
}

// ---------------------------------------------------------------- LCH

pub fn lab_to_lch(lab: Lab) -> Lch {
    Lch {
        l: lab.l,
        c: (lab.a * lab.a + lab.b * lab.b).sqrt(),
        h: lab.b.atan2(lab.a).to_degrees().rem_euclid(360.0),
    }
}

pub fn lch_to_lab(lch: Lch) -> Lab {
    let rad = lch.h.to_radians();
    Lab {
        l: lch.l,
        a: lch.c * rad.cos(),
        b: lch.c * rad.sin(),
    }
}

/// 色相最短路径插值（用于多色标手选场景；当前自动生成的相邻色标只差 10°，不走这里）。
#[cfg(test)]
pub fn lerp_hue(a: f32, b: f32, t: f32) -> f32 {
    let d = (b - a).rem_euclid(360.0);
    let d = if d > 180.0 { d - 360.0 } else { d };
    (a + d * t).rem_euclid(360.0)
}

// ---------------------------------------------------------------- 输出

/// OKLab → sRGB（u8）。越界时**保持 L 与 h 不变，二分降低 C**，不逐通道 clamp（那会偏色）。
pub fn oklab_to_srgb(lab: Lab) -> [u8; 4] {
    let lin = oklab_to_linear(lab);
    let lin = if in_gamut(lin) {
        lin
    } else {
        let lch = lab_to_lch(lab);
        let (mut lo, mut hi) = (0.0_f32, lch.c);
        for _ in 0..24 {
            let mid = (lo + hi) * 0.5;
            if in_gamut(oklab_to_linear(lch_to_lab(Lch { c: mid, ..lch }))) {
                lo = mid;
            } else {
                hi = mid;
            }
        }
        oklab_to_linear(lch_to_lab(Lch { c: lo, ..lch }))
    };
    let q = |v: f32| (linear_to_srgb(v.clamp(0.0, 1.0)) * 255.0).round().clamp(0.0, 255.0) as u8;
    [q(lin[0]), q(lin[1]), q(lin[2]), 255]
}

/// 两色在 OKLab 中插值；alpha 按**预乘语义**处理（颜色按 alpha 加权平均）。
/// 透明端不参与颜色——否则透明渐变中段会发黑变脏。
pub fn mix_oklab(a: [u8; 4], b: [u8; 4], t: f32) -> [u8; 4] {
    let t = t.clamp(0.0, 1.0);
    let (la, lb) = (srgb_to_oklab(a), srgb_to_oklab(b));
    let (aa, ab) = (a[3] as f32 / 255.0, b[3] as f32 / 255.0);
    let (wa, wb) = (aa * (1.0 - t), ab * t);
    let w = wa + wb;
    let lab = if w < 1e-4 {
        // 两端都透明：颜色无所谓，保持端点即可
        la
    } else {
        Lab {
            l: (la.l * wa + lb.l * wb) / w,
            a: (la.a * wa + lb.a * wb) / w,
            b: (la.b * wa + lb.b * wb) / w,
        }
    };
    let mut out = oklab_to_srgb(lab);
    out[3] = ((aa + (ab - aa) * t) * 255.0).round() as u8;
    out
}

/// 一维有序抖动（Bayer 4×4 的一行）：量化到 u8 前加 ±0.5/255，打散色带。
/// 渐变是沿参数的 1D 函数，按 stop 序号取相位即可等价于沿等值线的像素抖动。
const BAYER4: [i8; 16] = [0, 8, 2, 10, 12, 4, 14, 6, 3, 11, 1, 9, 15, 7, 13, 5];

fn dither(color: [u8; 4], index: usize) -> [u8; 4] {
    let step = BAYER4[index % 16] as f32 / 16.0 - 0.5; // -0.5..0.5
    // 量化前 ±0.5 的有序抖动：正值进位、负值退位，段间交替，平均不偏色。
    // （之前写的是 `(step * 0.5).round()`，恒为 0，等于没抖。）
    let off = if step >= 0.0 { 1 } else { -1 };
    let mut out = color;
    for ch in out.iter_mut().take(3) {
        *ch = (*ch as i32 + off).clamp(0, 255) as u8;
    }
    out
}

/// 把一组色标按 `n` 段在 OKLab 里重采样（含抖动）。
/// `tiny-skia` 的渐变只在相邻 stop 间做 sRGB 线性插值，所以必须在这里就把感知插值铺开。
pub fn sample_stops(stops: &[(f32, [u8; 4])], n: usize) -> Vec<(f32, [u8; 4])> {
    let mut out = Vec::with_capacity(n + 1);
    for i in 0..=n {
        let t = i as f32 / n as f32;
        out.push((t, dither(color_at(stops, t), i)));
    }
    out
}

/// 取渐变在参数 `t` 处的颜色（OKLab 分段插值）。
pub fn color_at(stops: &[(f32, [u8; 4])], t: f32) -> [u8; 4] {
    if stops.is_empty() {
        return [0, 0, 0, 255];
    }
    if t <= stops[0].0 {
        return stops[0].1;
    }
    for w in stops.windows(2) {
        let ((t0, c0), (t1, c1)) = (w[0], w[1]);
        if t <= t1 {
            let span = (t1 - t0).max(1e-6);
            return mix_oklab(c0, c1, (t - t0) / span);
        }
    }
    stops[stops.len() - 1].1
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: [u8; 4], b: [u8; 4], tol: i32) -> bool {
        (0..3).all(|i| (a[i] as i32 - b[i] as i32).abs() <= tol)
    }

    /// 往返：sRGB → OKLab → sRGB 基本还原（量化误差 ≤ 2）。
    #[test]
    fn srgb_oklab_roundtrip() {
        for c in [
            [229, 57, 53, 255],
            [251, 140, 0, 255],
            [30, 136, 229, 255],
            [67, 160, 71, 255],
            [0, 0, 0, 255],
            [255, 255, 255, 255],
            [128, 128, 128, 255],
        ] {
            let back = oklab_to_srgb(srgb_to_oklab(c));
            assert!(close(back, c, 2), "{c:?} → {back:?}");
        }
    }

    /// 中点绝不能发灰：红→橙的 OKLab 中点色度（chroma）≥ 两端较小者的 80%。
    /// （这正是首版在 sRGB 插值 + 手写色标时翻车的地方。）
    #[test]
    fn midpoint_keeps_chroma() {
        let lines = [
            ([229, 57, 53, 255], [251, 140, 0, 255]),
            ([30, 136, 229, 255], [0, 172, 193, 255]),
            ([142, 36, 170, 255], [216, 27, 96, 255]),
        ];
        for (a, b) in lines {
            let mid = mix_oklab(a, b, 0.5);
            let c = |x: [u8; 4]| lab_to_lch(srgb_to_oklab(x)).c;
            let floor = c(a).min(c(b)) * 0.8;
            assert!(c(mid) >= floor, "{a:?}→{b:?} 中点掉色度：{:?} → {}", mid, c(mid));
        }
    }

    /// 越界时保持 L / h，二分降 C：结果必须在色域内且色相基本不变。
    #[test]
    fn out_of_gamut_maps_by_chroma() {
        let wild = Lab {
            l: 0.7,
            a: 0.5,
            b: -0.4,
        };
        let out = oklab_to_srgb(wild);
        let back = srgb_to_oklab(out);
        let lch = lab_to_lch(back);
        let want = lab_to_lch(wild);
        assert!((lch.l - want.l).abs() < 0.05, "亮度应尽量保持：{} vs {}", lch.l, want.l);
        assert!((lch.h - want.h).abs() < 12.0, "色相应基本保持：{} vs {}", lch.h, want.h);
        assert!(lch.c < want.c, "应该只降色度");
    }

    /// 色相最短路径：350° → 10° 应走 +20°，不是 -340°。
    #[test]
    fn hue_takes_short_path() {
        let h = lerp_hue(350.0, 10.0, 0.5);
        assert!(
            (h - 0.0).abs() < 1e-3 || (h - 360.0).abs() < 1e-3,
            "中点应在 0° 附近：{h}"
        );
    }

    /// 采样：0 与 1 端点保持原色，段内是中点的感知插值（不是 sRGB 直线）。
    #[test]
    fn sampled_stops_keep_endpoints() {
        let stops = [(0.0, [229, 57, 53, 255]), (1.0, [251, 140, 0, 255])];
        let out = sample_stops(&stops, 16);
        assert_eq!(out.len(), 17);
        assert!(close(out[0].1, [229, 57, 53, 255], 2));
        assert!(close(out[16].1, [251, 140, 0, 255], 2));
        // 中点不是两端 sRGB 线性平均
        let naive = [((229 + 251) / 2) as u8, ((57 + 140) / 2) as u8, 26, 255];
        let mid = color_at(&stops, 0.5);
        assert!(
            !close(mid, naive, 1),
            "OKLab 中点不应等于 sRGB 线性平均：{mid:?} vs {naive:?}"
        );
    }

    /// 预乘 alpha 语义：透明端与另一端同色时，中间不应发黑。
    #[test]
    fn transparent_end_does_not_darken() {
        let a = [229, 57, 53, 255];
        let b = [229, 57, 53, 0];
        let mid = mix_oklab(a, b, 0.5);
        assert!(close(mid, a, 3), "中点颜色应与同色端一致（只降 alpha）：{mid:?}");
        assert!(mid[3] > 100 && mid[3] < 160, "alpha 应居中：{}", mid[3]);
    }
}
