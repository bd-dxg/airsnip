// SPDX-License-Identifier: GPL-3.0-only
//! 长截图状态条绘制（M4）：与截图工具栏同风格 —— tiny-skia 圆角矩形 + 主题调色板 +
//! DirectWrite 文字。分层窗口提交（见 `window`）。
//!
//! 布局：[进度文字 ......] [撤销] [完成] [取消]
//! 尺寸基准为 100% DPI，经 `Ui` 按显示器 DPI 缩放（CAP-5）。

use tiny_skia::{Pixmap, Transform};

use crate::editor::Point;
use crate::overlay::geometry::SelRect;
use crate::overlay::shape::{paint_of, rounded_path, rounded_rect};
use crate::overlay::ui::Ui;
use crate::render::icon::{self, Icon};
use crate::render::text as dwrite;
use crate::theme::Palette;

/// 状态条高度基准值（物理像素）。
const BAR_H_BASE: f32 = 40.0;
/// 状态条最小宽度基准值。
const BAR_MIN_W_BASE: f32 = 420.0;
/// 状态条最大宽度基准值。
const BAR_MAX_W_BASE: f32 = 760.0;
/// 按钮边长基准值。
const BTN_BASE: f32 = 28.0;
/// 内边距基准值。
const PAD_BASE: f32 = 6.0;
/// 圆角半径基准值。
const RADIUS_BASE: f32 = 8.0;
/// 文字字号基准值。
const FONT_BASE: f32 = 14.0;

/// 状态条上的按钮。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Button {
    /// 撤销最后一段。
    Undo,
    /// 完成。
    Done,
    /// 取消。
    Cancel,
}

/// 状态条命中区信息。
pub struct Bar {
    /// 整条矩形（屏幕坐标）。
    pub rect: SelRect,
    /// 界面缩放（绘制时的圆角与字号按它缩放）。
    pub ui: Ui,
    /// 按钮命中区：(矩形, 按钮)。
    hits: Vec<(SelRect, Button)>,
}

impl Bar {
    /// 命中测试。`x`/`y` 为**客户区坐标**（相对状态条左上角）——
    /// `WM_LBUTTONDOWN` 的 lparam 就是这个坐标系，不能再减 `rect` 原点。
    pub fn hit(&self, x: i32, y: i32) -> Option<Button> {
        self.hits
            .iter()
            .find(|(r, _)| x >= r.x && x < r.x + r.w && y >= r.y && y < r.y + r.h)
            .map(|(_, b)| *b)
    }
}

/// 状态条要显示的状态。
pub struct State {
    /// 是否可撤销。
    pub can_undo: bool,
    /// 提示文字（拼接失败等）。
    pub hint: &'static str,
}

/// 计算状态条几何（条放视口下方，空间不足则上方）。
pub fn layout(vp_x: i32, vp_w: u32, vp_y: i32, vp_h: u32, screen_h: i32, ui: Ui) -> Bar {
    let bar_h = ui.px(BAR_H_BASE);
    let btn = ui.px(BTN_BASE);
    let (pad, gap, margin) = (ui.px(PAD_BASE), ui.px(4.0), ui.px(6.0));
    let bar_w = (vp_w as i32).clamp(ui.px(BAR_MIN_W_BASE), ui.px(BAR_MAX_W_BASE));
    let x = vp_x + ((vp_w as i32) - bar_w) / 2;
    let below = vp_y + vp_h as i32 + margin;
    let y = if below + bar_h <= screen_h {
        below
    } else {
        (vp_y - bar_h - margin).max(0)
    };
    // 按钮从右往左排：取消、完成、撤销（命中区高度用整条高，点偏上下也能命中）
    let mut hits = Vec::new();
    let mut bx = bar_w - pad - btn;
    for b in [Button::Cancel, Button::Done, Button::Undo] {
        hits.push((
            SelRect {
                x: bx,
                y: 0,
                w: btn,
                h: bar_h,
            },
            b,
        ));
        bx -= btn + gap;
    }
    Bar {
        rect: SelRect {
            x,
            y,
            w: bar_w,
            h: bar_h,
        },
        ui,
        hits,
    }
}

/// 把状态条绘制到像素图（尺寸 = 条宽 × 条高）。
pub fn render(pixmap: &mut Pixmap, bar: &Bar, state: &State, p: &Palette) {
    let ui = bar.ui;
    let radius = ui.f(RADIUS_BASE);
    let (w, h) = (pixmap.width() as i32, pixmap.height() as i32);
    let full = SelRect { x: 0, y: 0, w, h };
    rounded_rect(pixmap, full, radius, p.bg, Some(p.border));
    // 顶部强调条（2px，与工具栏选中色呼应）
    let accent = SelRect {
        x: 0,
        y: 0,
        w,
        h: ui.px(3.0),
    };
    if let Some(path) = rounded_path(accent, radius) {
        pixmap.fill_path(
            &path,
            &paint_of(p.accent),
            tiny_skia::FillRule::Winding,
            Transform::identity(),
            None,
        );
    }

    // 右侧按钮：图标与覆盖层工具栏同源（`render::icon`），不再各画一套
    for (r, button) in &bar.hits {
        let (glyph, color) = match button {
            Button::Undo => (Icon::Undo, if state.can_undo { p.icon } else { p.icon_dim }),
            Button::Done => (Icon::Check, p.icon),
            Button::Cancel => (Icon::Cross, p.icon),
        };
        let cx = r.x as f32 + r.w as f32 / 2.0;
        let cy = r.y as f32 + r.h as f32 / 2.0;
        icon::draw(pixmap, glyph, cx, cy, r.w.min(r.h) as f32, color);
    }

    // 中间提示文字（像素数不再显示：右侧预览窗已给出拼接结果，LNG-3）
    let text = if state.hint.is_empty() {
        "滚轮滚动页面 · Enter 完成"
    } else {
        state.hint
    };
    let tx = ui.px(PAD_BASE) as f32;
    let spec = dwrite::FontSpec::new(ui.f(FONT_BASE));
    let (tw, th) = dwrite::measure(text, &spec);
    let ty = (h as f32 - th) / 2.0;
    // 文字不超出撤销按钮左侧
    let undo_x = bar
        .hits
        .iter()
        .find(|(_, b)| *b == Button::Undo)
        .map(|(r, _)| r.x)
        .unwrap_or(w);
    if tx + tw < undo_x as f32 - ui.f(4.0) {
        dwrite::draw(pixmap, Point::new(tx, ty), text, &spec, p.text, Transform::identity());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base() -> Ui {
        Ui::from_dpi(96)
    }

    #[test]
    fn layout_hits_all_buttons() {
        let ui = base();
        let bar = layout(100, 800, 100, 600, 1080, ui);
        let (bar_h, btn, pad, gap) = (ui.px(BAR_H_BASE), ui.px(BTN_BASE), ui.px(PAD_BASE), ui.px(4.0));
        let cy = bar_h / 2; // 客户区坐标（窗口 = 状态条矩形）
        // 右侧三个按钮：取消在最右
        let cancel = bar.hit(bar.rect.w - pad - btn / 2, cy);
        assert_eq!(cancel, Some(Button::Cancel));
        // 完成在取消左侧
        let done = bar.hit(bar.rect.w - pad - btn - gap - btn / 2, cy);
        assert_eq!(done, Some(Button::Done));
        // 撤销在完成左侧
        let undo = bar.hit(bar.rect.w - pad - (btn + gap) * 2 - btn / 2, cy);
        assert_eq!(undo, Some(Button::Undo));
        // 左侧空白（原模式按钮位置）无命中
        assert_eq!(bar.hit(pad + btn / 2, cy), None);
        // 中间空白无命中
        assert_eq!(bar.hit(bar.rect.w / 2, cy), None);
    }

    #[test]
    fn bar_falls_back_above_when_no_room_below() {
        let bar = layout(100, 800, 900, 160, 1080, base());
        assert!(bar.rect.y + bar.rect.h <= 900, "视口下方无空间时应放上方");
    }

    #[test]
    fn render_produces_opaque_panel() {
        let bar = layout(0, 600, 0, 400, 1080, base());
        let mut px = Pixmap::new(bar.rect.w as u32, bar.rect.h as u32).unwrap();
        let state = State {
            can_undo: true,
            hint: "",
        };
        render(&mut px, &bar, &state, &crate::theme::LIGHT);
        // 中心像素应为不透明背景
        let idx = ((bar.rect.h / 2) as u32 * px.width() + (px.width() / 2)) as usize * 4;
        assert!(px.data()[idx + 3] > 0, "状态条中心应有不透明像素");
    }

    /// 高 DPI 下状态条整体放大，按钮命中区同步（CAP-5）。
    #[test]
    fn bar_scales_with_dpi() {
        let base_bar = layout(100, 1600, 100, 600, 1080, base());
        let hidpi = layout(100, 1600, 100, 600, 2160, Ui::from_dpi(192));
        assert_eq!(hidpi.rect.h, base_bar.rect.h * 2, "状态条高度应翻倍");
        assert_eq!(hidpi.rect.w, base_bar.rect.w * 2, "状态条宽度应翻倍");
        // 最右侧按钮中心仍可命中
        let x = hidpi.rect.w - hidpi.ui.px(PAD_BASE) - hidpi.ui.px(BTN_BASE) / 2;
        assert_eq!(hidpi.hit(x, hidpi.rect.h / 2), Some(Button::Cancel));
    }
}
