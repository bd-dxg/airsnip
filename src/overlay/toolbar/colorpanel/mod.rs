// SPDX-License-Identifier: GPL-3.0-only
//! 取色面板（EDT-7，issue #73）：双击工具栏色块展开的自绘取色浮层，改写该色块。
//! 内容：色相条 + 饱和度 / 明度方块 + 快捷色块。布局 / 命中在本模块，绘制见 `paint`，
//! 交互落在 `interact`。仿 `toolbar::fontlist`（同为主线程内自绘浮层）。
//! 尺寸基准为 100% DPI，经 `Ui` 缩放（CAP-5）。

mod interact;
mod paint;
pub(in crate::overlay) use paint::render;

use crate::editor::palette::{QUICK, hsv_to_rgb, rgb_to_hsv};
use crate::overlay::geometry::{SelRect, in_rect};
use crate::overlay::ui::Ui;

/// 面板宽度基准值。
const W_BASE: f32 = 208.0;
/// 内边距基准值。
const PAD_BASE: f32 = 8.0;
/// 元素间距基准值。
const GAP_BASE: f32 = 8.0;
/// 色相条高度基准值。
const HUE_H_BASE: f32 = 14.0;
/// 饱和度 / 明度方块高度基准值。
const SV_H_BASE: f32 = 104.0;
/// 快捷色块边长基准值。
const SW_BASE: f32 = 24.0;
/// 快捷色块间距基准值。
const SW_GAP_BASE: f32 = 8.0;
/// 快捷色块列数（行数由 `QUICK` 数量推出）。
const COLS: usize = 6;

/// 面板内的命中区。
#[derive(Clone, Copy, PartialEq, Debug)]
pub(in crate::overlay) enum Hit {
    /// 色相条。
    Hue,
    /// 饱和度 / 明度方块。
    Sv,
    /// 快捷色块下标。
    Quick(usize),
}

/// 打开中的取色面板状态。
pub(in crate::overlay) struct ColorPanel {
    pub(in crate::overlay) rect: SelRect,
    ui: Ui,
    /// 色相（0..360）。
    hue: f32,
    /// 饱和度（0..1）。
    sat: f32,
    /// 明度（0..1）。
    val: f32,
    /// 正在拖动的滑块（色相 / 明度方块）；快捷色块为单击不拖动。
    drag: Option<Hit>,
    /// 悬停的快捷色块下标。
    hover: Option<usize>,
}

impl ColorPanel {
    /// 从被双击的色块与工具栏位置展开：水平居中于色块，优先显示在工具栏上方，
    /// 上方空间不足时翻到下方；并钳回屏幕内。
    pub(in crate::overlay) fn open(
        anchor: SelRect,
        bar: SelRect,
        color: [u8; 4],
        screen_w: i32,
        screen_h: i32,
        ui: Ui,
    ) -> Self {
        let (h, s, v) = rgb_to_hsv(color);
        let (pad, gap) = (ui.px(PAD_BASE), ui.px(GAP_BASE));
        let (hue_h, sv_h) = (ui.px(HUE_H_BASE), ui.px(SV_H_BASE));
        let rows = QUICK.len().div_ceil(COLS) as i32;
        let quick_h = rows * ui.px(SW_BASE) + (rows - 1) * ui.px(SW_GAP_BASE);
        let (w, ph) = (ui.px(W_BASE), pad + hue_h + gap + sv_h + gap + quick_h + pad);
        let margin = ui.px(4.0);
        let x = (anchor.x + (anchor.w - w) / 2).clamp(margin, (screen_w - w - margin).max(margin));
        let above = bar.y - ui.px(6.0) - ph;
        let y = if above >= margin {
            above
        } else {
            (bar.y + bar.h + ui.px(6.0)).min((screen_h - ph - margin).max(margin))
        };
        Self {
            rect: SelRect { x, y, w, h: ph },
            ui,
            hue: h,
            sat: s,
            val: v,
            drag: None,
            hover: None,
        }
    }

    /// 界面缩放（绘制子模块读取圆角与尺寸）。
    pub(in crate::overlay) fn ui(&self) -> Ui {
        self.ui
    }

    /// 当前 HSV（色相 0..360，饱和 / 明度 0..1）。
    pub(in crate::overlay) fn hsv(&self) -> (f32, f32, f32) {
        (self.hue, self.sat, self.val)
    }

    /// 当前颜色。
    pub(in crate::overlay) fn color(&self) -> [u8; 4] {
        hsv_to_rgb(self.hue, self.sat, self.val)
    }

    fn pad(&self) -> i32 {
        self.ui.px(PAD_BASE)
    }

    fn gap(&self) -> i32 {
        self.ui.px(GAP_BASE)
    }

    fn inner_w(&self) -> i32 {
        self.rect.w - self.pad() * 2
    }

    pub(in crate::overlay) fn hue_rect(&self) -> SelRect {
        SelRect {
            x: self.rect.x + self.pad(),
            y: self.rect.y + self.pad(),
            w: self.inner_w(),
            h: self.ui.px(HUE_H_BASE),
        }
    }

    pub(in crate::overlay) fn sv_rect(&self) -> SelRect {
        let hue = self.hue_rect();
        SelRect {
            x: hue.x,
            y: hue.y + hue.h + self.gap(),
            w: self.inner_w(),
            h: self.ui.px(SV_H_BASE),
        }
    }

    pub(in crate::overlay) fn quick_rect(&self, i: usize) -> SelRect {
        let (sw, sg) = (self.ui.px(SW_BASE), self.ui.px(SW_GAP_BASE));
        let row_w = COLS as i32 * sw + (COLS as i32 - 1) * sg;
        let sv = self.sv_rect();
        let x0 = self.rect.x + self.pad() + (self.inner_w() - row_w) / 2;
        let y0 = sv.y + sv.h + self.gap();
        SelRect {
            x: x0 + (i % COLS) as i32 * (sw + sg),
            y: y0 + (i / COLS) as i32 * (sw + sg),
            w: sw,
            h: sw,
        }
    }

    /// 鼠标是否在面板内。
    pub(in crate::overlay) fn contains(&self, x: i32, y: i32) -> bool {
        in_rect(self.rect, x, y)
    }

    /// 命中面板内控件；落在内边距 / 空白处返回 None。
    pub(in crate::overlay) fn hit(&self, x: i32, y: i32) -> Option<Hit> {
        if !self.contains(x, y) {
            return None;
        }
        if in_rect(self.hue_rect(), x, y) {
            return Some(Hit::Hue);
        }
        if in_rect(self.sv_rect(), x, y) {
            return Some(Hit::Sv);
        }
        (0..QUICK.len())
            .find(|&i| in_rect(self.quick_rect(i), x, y))
            .map(Hit::Quick)
    }

    /// 按下：滑块开始拖动并立即取样，快捷色块直接取色。
    pub(in crate::overlay) fn press(&mut self, hit: Hit, x: i32, y: i32) {
        match hit {
            Hit::Hue | Hit::Sv => {
                self.drag = Some(hit);
                self.drag_to(x, y);
            }
            Hit::Quick(i) => self.set_color(QUICK[i]),
        }
    }

    /// 是否正在拖动滑块（鼠标移出面板仍继续）。
    pub(in crate::overlay) fn dragging(&self) -> bool {
        self.drag.is_some()
    }

    /// 拖动取样。
    pub(in crate::overlay) fn drag_to(&mut self, x: i32, y: i32) {
        match self.drag {
            Some(Hit::Hue) => {
                let r = self.hue_rect();
                self.hue = (((x - r.x) as f32 / r.w.max(1) as f32) * 360.0).clamp(0.0, 360.0);
            }
            Some(Hit::Sv) => {
                let r = self.sv_rect();
                self.sat = ((x - r.x) as f32 / r.w.max(1) as f32).clamp(0.0, 1.0);
                self.val = (1.0 - (y - r.y) as f32 / r.h.max(1) as f32).clamp(0.0, 1.0);
            }
            _ => {}
        }
    }

    /// 结束拖动。
    pub(in crate::overlay) fn end_drag(&mut self) {
        self.drag = None;
    }

    /// 用具体颜色更新 HSV（快捷色块 / 初值）。
    pub(in crate::overlay) fn set_color(&mut self, c: [u8; 4]) {
        let (h, s, v) = rgb_to_hsv(c);
        self.hue = h;
        self.sat = s;
        self.val = v;
    }

    /// 更新悬停的快捷色块；返回是否变化。
    pub(in crate::overlay) fn hover_at(&mut self, x: i32, y: i32) -> bool {
        let hover = match self.hit(x, y) {
            Some(Hit::Quick(i)) => Some(i),
            _ => None,
        };
        let changed = hover != self.hover;
        self.hover = hover;
        changed
    }

    /// 悬停的快捷色块下标（绘制高亮用）。
    pub(in crate::overlay) fn hover(&self) -> Option<usize> {
        self.hover
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base() -> Ui {
        Ui::from_dpi(96)
    }

    fn anchor(x: i32, y: i32) -> SelRect {
        SelRect { x, y, w: 22, h: 22 }
    }

    fn bar(x: i32, y: i32) -> SelRect {
        SelRect { x, y, w: 560, h: 34 }
    }

    /// 优先展开在工具栏上方，且整块面板落在屏幕内。
    #[test]
    fn prefers_above_and_stays_inside() {
        let (sw, sh) = (1366, 768);
        let p = ColorPanel::open(anchor(300, 700), bar(300, 676), [142, 36, 170, 255], sw, sh, base());
        assert!(p.rect.y + p.rect.h <= 676, "应展开在工具栏上方：{:?}", p.rect);
        assert!(p.rect.x >= 0 && p.rect.x + p.rect.w <= sw);

        // 工具栏贴顶：翻到下方
        let p2 = ColorPanel::open(anchor(300, 10), bar(300, 4), [142, 36, 170, 255], sw, sh, base());
        assert!(p2.rect.y >= 38, "应展开在工具栏下方：{:?}", p2.rect);
        assert!(p2.rect.y + p2.rect.h <= sh);
    }

    /// 命中：色相条 / 明度方块 / 快捷色块各归其位。
    #[test]
    fn hit_regions_map() {
        let p = ColorPanel::open(anchor(300, 700), bar(300, 676), [142, 36, 170, 255], 1366, 768, base());
        let (hue, sv, q0) = (p.hue_rect(), p.sv_rect(), p.quick_rect(0));
        assert_eq!(p.hit(hue.x + 3, hue.y + 3), Some(Hit::Hue));
        assert_eq!(p.hit(sv.x + 3, sv.y + 3), Some(Hit::Sv));
        assert_eq!(p.hit(q0.x + 3, q0.y + 3), Some(Hit::Quick(0)));
        assert_eq!(p.hit(p.rect.x - 5, p.rect.y), None, "面板外不命中");
    }

    /// 拖动明度方块：右上角为纯色（高饱和高明度），左下角为黑 / 白。
    #[test]
    fn sv_drag_samples_saturation_and_value() {
        let mut p = ColorPanel::open(anchor(300, 700), bar(300, 676), [229, 57, 53, 255], 1366, 768, base());
        let sv = p.sv_rect();
        p.press(Hit::Sv, sv.x + sv.w - 1, sv.y + 1);
        let (_, s, v) = p.hsv();
        assert!(s > 0.9 && v > 0.9, "右上角应接近纯色：s={s} v={v}");
        p.drag_to(sv.x + 1, sv.y + sv.h - 1);
        let (_, s, v) = p.hsv();
        assert!(s < 0.1 && v < 0.1, "左下角应接近黑：s={s} v={v}");
        assert!(p.dragging());
        p.end_drag();
        assert!(!p.dragging());
    }

    /// 色相条取色：左端为红（h≈0），颜色随之改变。
    #[test]
    fn hue_drag_changes_hue() {
        let mut p = ColorPanel::open(anchor(300, 700), bar(300, 676), [0, 0, 0, 255], 1366, 768, base());
        let hue = p.hue_rect();
        p.press(Hit::Hue, hue.x, hue.y + 2);
        assert!(p.hsv().0.abs() < 1e-3, "左端色相应为 0");
        p.drag_to(hue.x + hue.w / 2, hue.y + 2);
        assert!((p.hsv().0 - 180.0).abs() < 5.0, "中点色相约 180：{}", p.hsv().0);
    }

    /// 高 DPI 下面板与命中区同步放大（CAP-5）。
    #[test]
    fn panel_scales_with_dpi() {
        let b = ColorPanel::open(anchor(300, 700), bar(300, 676), [0, 0, 0, 255], 1366, 768, base());
        let d = ColorPanel::open(
            anchor(600, 1400),
            bar(600, 1352),
            [0, 0, 0, 255],
            2732,
            1536,
            Ui::from_dpi(192),
        );
        assert_eq!(d.rect.w, b.rect.w * 2, "200% 下面板应加宽一倍");
        assert_eq!(d.rect.h, b.rect.h * 2, "200% 下面板应加高一倍");
    }
}
