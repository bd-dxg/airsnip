// SPDX-License-Identifier: GPL-3.0-only
//! 悬浮提示气泡的几何与绘制（EDT-10）：从 `tip` 拆出，控制单文件规模；
//! 悬停状态机与文案见 `super`。

use tiny_skia::{Pixmap, Transform};

use crate::editor::Point;
use crate::overlay::geometry::SelRect;
use crate::overlay::shape::rounded_rect;
use crate::overlay::ui::Ui;
use crate::render::text::{self as dwrite, FontSpec};
use crate::theme::Palette;

use super::Bubble;

/// 气泡与工具栏的间距。
const GAP_BASE: f32 = 6.0;
/// 气泡水平内边距。
const PAD_X_BASE: f32 = 10.0;
/// 气泡垂直内边距。
const PAD_Y_BASE: f32 = 5.0;
/// 气泡圆角半径。
const RADIUS_BASE: f32 = 6.0;
/// 气泡字号（与工具栏第二行标签同档）。
const FONT_BASE: f32 = 13.0;
/// 离屏幕边缘的最小留白。
const MARGIN_BASE: f32 = 4.0;

/// 气泡几何：水平对齐 `anchor` 中心（钳制在屏幕内），垂直贴 `vertical` 外缘
/// （下方优先，塞不下翻到上方）。
/// 按钮的垂直参照是**整条工具栏**而非按钮所在行：贴行内会被第二行的字体控件挡住（ADR 0020）。
/// 圆角控制柄没有工具栏可言，水平与垂直都参照手柄自身。
pub(crate) fn bubble_rect(anchor: SelRect, vertical: SelRect, text: &str, vw: i32, vh: i32, ui: Ui) -> SelRect {
    let spec = FontSpec::new(ui.f(FONT_BASE));
    let (tw, th) = dwrite::measure(text, &spec);
    let (pad_x, pad_y) = (ui.f(PAD_X_BASE), ui.f(PAD_Y_BASE));
    let w = (tw + pad_x * 2.0).ceil() as i32;
    let h = (th + pad_y * 2.0).ceil() as i32;
    let gap = ui.px(GAP_BASE);
    let margin = ui.px(MARGIN_BASE);
    let x = (anchor.x + (anchor.w - w) / 2).clamp(margin, (vw - w - margin).max(margin));
    let below = vertical.y + vertical.h + gap;
    let y = if below + h + margin <= vh {
        below
    } else {
        (vertical.y - gap - h).max(margin)
    };
    SelRect { x, y, w, h }
}

/// 绘制气泡（屏幕显示层，不进导出烘焙）。
pub(crate) fn draw(pixmap: &mut Pixmap, bubble: &Bubble, p: &Palette, ui: Ui) {
    // 背景改不透明：半透明底叠在暗图上会让小字发灰
    let bg = [p.bg[0], p.bg[1], p.bg[2], 255];
    rounded_rect(pixmap, bubble.rect, ui.f(RADIUS_BASE), bg, Some(p.border));
    let spec = FontSpec::new(ui.f(FONT_BASE));
    let (tw, th) = dwrite::measure(&bubble.text, &spec);
    let x = bubble.rect.x as f32 + (bubble.rect.w as f32 - tw) / 2.0;
    let y = bubble.rect.y as f32 + (bubble.rect.h as f32 - th) / 2.0;
    dwrite::draw(
        pixmap,
        Point::new(x, y),
        &bubble.text,
        &spec,
        p.text,
        Transform::identity(),
    );
}
