// SPDX-License-Identifier: GPL-3.0-only
//! 多色标渐变（EDT-19）：色标在 **OKLCH** 里生成，插值在 **OKLab** 里做（见 `oklab`）。
//!
//! 用户口径：明亮纯净、不显脏；**一套渐变里就要有多种可辨识的颜色**（不是「红→橙」两段）；
//! 同色系 / 邻近色在色相环上顺路过渡；每套不同，上一次的结束色作为下一次的开始。
//!
//! 落到实现：
//! - 一套 = `STOPS`（5）个色标，色相从基准色一路顺推到 +`HUE_SPAN`（60°），
//!   相邻色标只差 15° —— 单段跨度小（不脏），整条能数出四五种颜色；
//! - L 只在中间轻微拱起（+0.06），C 基本不动：**不混白 / 不压暗**；
//! - 第 n 套用 `nth(base, n)` 直接算（基准色相 + n×`HUE_STEP`），不拿上一套终点当起点：
//!   后者经 sRGB u8 往返会逐套漂，步长越走越小（用户看到的「几乎不变色」）。
//!
//! 进度只存在运行时（`Overlay::gradient`），不写配置：重启后从当前色块重新起手。

use super::oklab::{Lch, lab_to_lch, lch_to_lab, oklab_to_srgb, peak_chroma, sample_stops, srgb_to_oklab};

/// 一套渐变的色标数量：赤橙黄绿青蓝紫，再加一个**闭合点**（回到起点色）。
/// 闭合是为了沿周长描边时首尾同色——否则矩形的左上角会有一道硬接缝。
pub const STOPS: usize = 8;

/// 一套渐变里的色相跨度（度）：360° 走完一圈并回到起点色，用于**闭合描边**。
pub const HUE_SPAN: f32 = 360.0;

/// 相邻对象之间色相前推的角度（度）：45° 让下一套从橙 / 黄…起头，不再每次都从红开始。
pub const HUE_STEP: f32 = 45.0;

/// 统一目标亮度：整套色标都落在这个感知亮度上。
/// 若跟着基准色（红 L≈0.55）走，黄绿青必然发浑发暗（用户反馈「过渡色浑浊」）。
const TARGET_L: f32 = 0.80;
/// 色度取该 (L, h) 下色域最大值的比例：贴边容易溢出或刺眼，留一点余量。
const CHROMA_K: f32 = 0.92;

/// 指定色相在统一亮度下的「鲜亮」版本：该 L 下能取到的最大色度 × 0.92。
///
/// 黄绿（h≈90°）额外抬亮：同一 L 下它在暗底上看起来最浑（对比度最低），
/// 抬 0.08 后与红 / 紫的「亮度感」才齐。
fn vivid(h: f32) -> [u8; 4] {
    let bump = ((h - 90.0).to_radians().cos()).max(0.0);
    let l = (TARGET_L + 0.08 * bump).min(0.96);
    let c = peak_chroma(l, h) * CHROMA_K;
    oklab_to_srgb(lch_to_lab(Lch { l, c, h }))
}

/// 低于此色度视作无彩色（白 / 灰 / 黑），色相没有意义。
const ACHROMATIC: f32 = 0.02;

/// 多色标渐变：`stops[0]` 起、`stops[STOPS-1]` 止，中间等距。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Gradient {
    pub stops: [[u8; 4]; STOPS],
}

impl Gradient {
    /// 第 `n` 套（从 `base` 起算，不累积）：整套递色相的起点 = 基准色相 + `n × HUE_STEP`。
    pub fn nth(base: [u8; 4], n: u32) -> Self {
        let lch = lab_to_lch(srgb_to_oklab(base));
        let hue0 = lch.h + HUE_STEP * n as f32;
        let to = |l: f32, c: f32, h: f32| {
            oklab_to_srgb(lch_to_lab(Lch {
                l: l.clamp(0.0, 1.0),
                c: c.max(0.0),
                h,
            }))
        };
        let mut stops = [[0u8; 4]; STOPS];
        // 相邻两套方向交替（正序 / 反序）：每套都是完整光谱，若只旋转起点，
        // 视觉上“都是一样的彩框”（用户反馈「固定了」）；反向一刷差异立刻可见。
        let dir = if n.is_multiple_of(2) { 1.0 } else { -1.0 };
        // 无彩色（白 / 灰 / 黑）：色相没意义，沿 L 做梯度（随套数轻微起伏以免每套一样）
        if lch.c < ACHROMATIC {
            let wob = ((n as f32) * 1.3).sin() * 0.05;
            for (i, s) in stops.iter_mut().enumerate() {
                let t = i as f32 / (STOPS - 1) as f32;
                *s = to(lch.l - 0.20 * t + wob * (1.0 - t), lch.c, lch.h);
            }
            return Self { stops };
        }
        for (i, s) in stops.iter_mut().enumerate() {
            let t = i as f32 / (STOPS - 1) as f32;
            // 色相绕色环等距推进（偶数套正序、奇数套反序）；亮度统一到 TARGET_L
            *s = vivid(hue0 + HUE_SPAN * t * dir);
        }
        Self { stops }
    }

    /// 首套（从当前纯色起手）。
    pub fn seed(color: [u8; 4]) -> Self {
        Self::nth(color, 0)
    }

    /// 代表色（工具栏光晕、文本 / 徽标对比度、纯色回退）：全光谱下取起点色最有代表性。
    pub fn representative(&self) -> [u8; 4] {
        self.stops[0]
    }

    /// 边缘色（起点）：工具栏渐变色块的径向渐变用它作外圈。
    pub fn edge(&self) -> [u8; 4] {
        self.stops[0]
    }

    /// 把全部色标的不透明度统一成 `alpha`。
    pub fn with_alpha(mut self, alpha: u8) -> Self {
        for s in &mut self.stops {
            s[3] = alpha;
        }
        self
    }

    /// 重采样成 `n` 段（位置, 颜色），在 OKLab 里插值并带抖动。
    /// 渲染层只认这个——`tiny-skia` 的 stop 间插值是 sRGB 线性，必须提前把感知插值铺开。
    pub fn sampled(&self, n: usize) -> Vec<(f32, [u8; 4])> {
        let last = (STOPS - 1) as f32;
        let stops: Vec<(f32, [u8; 4])> = self
            .stops
            .iter()
            .enumerate()
            .map(|(i, c)| (i as f32 / last, *c))
            .collect();
        sample_stops(&stops, n)
    }

    /// 取整条渐变在参数 `t`（0..1）处的颜色（OKLab 插值）：沿路径渐变描边逐段取色用。
    pub fn color_at(&self, t: f32) -> [u8; 4] {
        let last = (STOPS - 1) as f32;
        let stops: Vec<(f32, [u8; 4])> = self
            .stops
            .iter()
            .enumerate()
            .map(|(i, c)| (i as f32 / last, *c))
            .collect();
        crate::editor::oklab::color_at(&stops, t)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::editor::oklab::Lab;

    fn lab(c: [u8; 4]) -> Lab {
        srgb_to_oklab(c)
    }

    fn lch(c: [u8; 4]) -> Lch {
        lab_to_lch(lab(c))
    }

    fn dist(a: [u8; 4], b: [u8; 4]) -> i32 {
        (0..3).map(|i| (a[i] as i32 - b[i] as i32).abs()).sum::<i32>()
    }

    /// 首套起点 = 基准色相的**鲜亮版**（色相一致、亮度统一到 TARGET_L）。
    #[test]
    fn seed_starts_from_base_hue_bright() {
        let red = [229, 57, 53, 255];
        let g = Gradient::seed(red);
        let (h0, h1) = (lch(red).h, lch(g.stops[0]).h);
        assert!((h0 - h1).abs() < 2.0, "起点色相应等于基准：{h0} vs {h1}");
        assert!(
            (TARGET_L - 0.01..=TARGET_L + 0.10).contains(&lch(g.stops[0]).l),
            "起点应落在明亮区间：{}",
            lch(g.stops[0]).l
        );
        assert_eq!(g.stops.len(), STOPS);
    }

    /// 一套渐变里必须有多种可辨的颜色：相邻色标 RGB 差 ≥ 12，首尾差 ≥ 60。
    #[test]
    fn one_gradient_holds_several_colors() {
        let base = [229, 57, 53, 255];
        for n in 0..6u32 {
            let g = Gradient::nth(base, n);
            for w in g.stops.windows(2) {
                assert!(dist(w[0], w[1]) >= 12, "第 {n} 套相邻色标太像：{:?} / {:?}", w[0], w[1]);
            }
            assert!(
                dist(g.stops[0], g.stops[STOPS / 2]) >= 60,
                "第 {n} 套只走了半圈？{:?}",
                g.stops
            );
            assert!(
                dist(g.stops[0], g.stops[STOPS - 1]) <= 6,
                "第 {n} 套首尾应闭合：{:?}",
                g.stops
            );
        }
    }

    /// 整套色相跨度 = 360°，且首尾回绕到同一色相（闭合，沿周长描边无接缝）。
    #[test]
    fn stops_span_expected_hue_range() {
        let g = Gradient::seed([229, 57, 53, 255]);
        let (h0, h1) = (lch(g.stops[0]).h, lch(g.stops[STOPS - 1]).h);
        let d = (h1 - h0).rem_euclid(360.0);
        assert!(d < 2.0 || d > 358.0, "首尾应闭合（同一色相）：{h0} → {h1}");
        // 中点应在对侧
        let dm = (lch(g.stops[STOPS / 2]).h - h0).rem_euclid(360.0);
        assert!((150.0..=210.0).contains(&dm), "中点应在对侧：{dm}°");
    }

    /// 每套步长稳定：`nth(base, n)` 的起点色相 = 基准 + n×HUE_STEP（不漂）。
    #[test]
    fn nth_advances_by_fixed_step() {
        let base = [229, 57, 53, 255];
        let h0 = lch(base).h;
        for n in 0..12u32 {
            let h = lch(Gradient::nth(base, n).stops[0]).h;
            let want = (h0 + HUE_STEP * n as f32).rem_euclid(360.0);
            let d = (h - want).rem_euclid(360.0).min((want - h).rem_euclid(360.0));
            assert!(d <= 2.0, "第 {n} 套色相偏了：{h} vs {want}");
        }
    }

    /// 相邻两套必须「一眼不同」：起点差 ≥ 20，且色标顺序相反（正序 / 反序交替）。
    #[test]
    fn adjacent_sets_differ_visibly() {
        let base = [229, 57, 53, 255];
        for n in 0..8u32 {
            let a = Gradient::nth(base, n);
            let b = Gradient::nth(base, n + 1);
            assert!(
                dist(a.stops[0], b.stops[0]) >= 15,
                "第 {n}/{n}+1 套起点太像：{:?} vs {:?}",
                a.stops[0],
                b.stops[0]
            );
            // 一套的第一→第七与另一套的第七→第一走势相反（颜色序列倒过来）
            let pair = |g: &Gradient| lch(g.stops[0]).h;
            assert!((pair(&a) - pair(&b)).abs() >= 15.0, "第 {n} 套与下一套起点色相太近");
        }
    }

    /// 亮度不随套数累积下降（用户看到的「越来越脏」就是它）。
    /// 黄绿段会按色相额外抬亮，所以只断言「不暗于基准」，不要求每套完全同亮度。
    #[test]
    fn lightness_does_not_accumulate() {
        let base = [229, 57, 53, 255];
        for n in 1..=10u32 {
            let l = lch(Gradient::nth(base, n).stops[0]).l;
            assert!(l >= TARGET_L - 0.01, "第 {n} 套变暗了：{l}");
        }
    }

    /// 无彩色（白 / 灰）沿 L 出梯度，不会几个色标全同色。
    #[test]
    fn achromatic_gets_lightness_stops() {
        let g = Gradient::seed([255, 255, 255, 255]);
        assert_ne!(g.stops[0], g.stops[STOPS - 1]);
    }

    /// 采样：端点保持，段数正确，且中点与 sRGB 直线插值不同（证明走了 OKLab）。
    #[test]
    fn sampling_is_perceptual() {
        let g = Gradient::seed([229, 57, 53, 255]);
        let s = g.sampled(32);
        assert_eq!(s.len(), 33);
        assert!(dist(s[0].1, g.stops[0]) <= 4, "起点应保持（含 ±1 抖动）");
        assert!(dist(s[32].1, g.stops[STOPS - 1]) <= 4, "终点应保持（含 ±1 抖动）");
        let mid = g.sampled(2)[1].1;
        let naive = [
            ((g.stops[0][0] as u16 + g.stops[STOPS - 1][0] as u16) / 2) as u8,
            ((g.stops[0][1] as u16 + g.stops[STOPS - 1][1] as u16) / 2) as u8,
            ((g.stops[0][2] as u16 + g.stops[STOPS - 1][2] as u16) / 2) as u8,
            255,
        ];
        assert_ne!(mid[..3], naive[..3], "中点应与 sRGB 线性平均不同");
    }

    /// 统一 alpha：全部色标一起改。
    #[test]
    fn alpha_applies_to_all_stops() {
        let g = Gradient::seed([229, 57, 53, 255]).with_alpha(120);
        assert!(g.stops.iter().all(|s| s[3] == 120));
    }
}
