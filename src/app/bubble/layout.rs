// SPDX-License-Identifier: GPL-3.0-only
//! 气泡几何（OCR-8/OCR-9）：三段式，照着 Bob 的划词翻译窗口排——
//! 顶部淡图标行（图钉 / 关闭）、中间文本区、底部动作行（复制 + 识别语言）。
//! 后续接翻译时在动作行下面加分隔线与译文块即可，不用改结构。
//!
//! 尺寸以 1080p / 100% 为基准，经 `Ui` 按显示器缩放（与覆盖层同一套，ADR 0016）。

use crate::overlay::geometry::SelRect;
use crate::overlay::ui::Ui;

/// 卡片宽度（320 太窄、读长段文字频繁折行，2026-02 实测后放大）。
const W_BASE: f32 = 420.0;
/// 内边距（仅本模块用）。
const PAD_BASE: f32 = 14.0;
/// 圆角半径。
pub const RADIUS_BASE: f32 = 10.0;
/// 顶部图标行高度。
const TOP_BASE: f32 = 30.0;
/// 底部动作行高度。
const ACTION_BASE: f32 = 30.0;
/// 顶部行里的图标点击区边长。
const ICON_BASE: f32 = 26.0;
/// 右下角「自动复制」开关区域尺寸（文字 + 开关）。
const AUTO_COPY_W_BASE: f32 = 96.0;
const AUTO_COPY_H_BASE: f32 = 24.0;
/// 左下角「复制」按钮尺寸。
const COPY_W_BASE: f32 = 60.0;
const COPY_H_BASE: f32 = 24.0;
/// 开关轨道尺寸。
const TRACK_W_BASE: f32 = 28.0;
const TRACK_H_BASE: f32 = 16.0;
/// 文本区边框内缩（文字与边框的间距）。
const BODY_INSET_BASE: f32 = 4.0;
/// 文本区最小 / 最大高度（超出最大高度时文本区内部滚动）。
const MIN_BODY_BASE: f32 = 40.0;
const MAX_BODY_BASE: f32 = 400.0;

/// 气泡布局。
pub struct Layout {
    /// 窗口尺寸（= 卡片尺寸，无阴影留白）。
    pub w: i32,
    pub h: i32,
    /// 顶部图标行（图钉在左、关闭在右）。
    pub pin: SelRect,
    /// 关闭按钮。
    pub close: SelRect,
    /// 文本可视区。
    pub body: SelRect,
    /// 文本区 1px 边框（常态 `divider`，聚焦时强调色）。
    pub frame: SelRect,
    /// 左下角「复制」按钮（点击有反馈）。
    pub copy: SelRect,
    /// 右下角「自动复制」开关（文字 + 开关，整块可点）。
    pub auto_copy: SelRect,
}

/// 计算布局：`content_h` 为内容高度，最大不超过 `MAX_BODY_BASE`。
pub fn layout(ui: Ui, content_h: i32) -> Layout {
    layout_capped(ui, content_h, ui.px(MAX_BODY_BASE))
}

/// 同上，但最大高度可覆盖。
///
/// 选区靠近屏幕下方时，卡片可能比工作区还高——那时按屏幕可用高度收窄（`view::relayout` 传入），
/// 否则弹窗会溢出屏幕（OCR-7）。
pub fn layout_capped(ui: Ui, content_h: i32, max_body: i32) -> Layout {
    let pad = ui.px(PAD_BASE);
    let w = ui.px(W_BASE);
    let top_h = ui.px(TOP_BASE);
    let action_h = ui.px(ACTION_BASE);
    let body_h = content_h.clamp(ui.px(MIN_BODY_BASE), max_body.max(ui.px(MIN_BODY_BASE)));
    let icon = ui.px(ICON_BASE);

    let top = SelRect {
        x: pad,
        y: pad / 2,
        w: w - pad * 2,
        h: top_h,
    };
    let pin = SelRect {
        x: top.x,
        y: top.y + (top_h - icon) / 2,
        w: icon,
        h: icon,
    };
    let close = SelRect {
        x: top.x + top.w - icon,
        y: pin.y,
        w: icon,
        h: icon,
    };
    let body = SelRect {
        x: pad,
        y: top.y + top_h,
        w: w - pad * 2,
        h: body_h,
    };
    // 边框包住文本区，文字内缩一点，不贴线
    let inset = ui.px(BODY_INSET_BASE);
    let frame = SelRect {
        x: body.x - inset,
        y: body.y - inset,
        w: body.w + inset * 2,
        h: body.h + inset * 2,
    };
    let action_y = frame.y + frame.h + ui.px(4.0);
    let copy_h = ui.px(COPY_H_BASE);
    // 「复制」在左、「自动复制」开关在右（开关右边线与关闭图标对齐）
    let copy = SelRect {
        x: pad,
        y: action_y + (action_h - copy_h) / 2,
        w: ui.px(COPY_W_BASE),
        h: copy_h,
    };
    let auto_copy = SelRect {
        x: w - pad - ui.px(AUTO_COPY_W_BASE),
        y: action_y + (action_h - ui.px(AUTO_COPY_H_BASE)) / 2,
        w: ui.px(AUTO_COPY_W_BASE),
        h: ui.px(AUTO_COPY_H_BASE),
    };
    Layout {
        w,
        h: action_y + action_h + pad / 2,
        pin,
        close,
        body,
        frame,
        copy,
        auto_copy,
    }
}

/// 开关轨道矩形（在 `auto_copy` 区域最右，垂直居中）。
pub fn track_rect(ui: Ui, area: SelRect) -> SelRect {
    let tw = ui.px(TRACK_W_BASE);
    let th = ui.px(TRACK_H_BASE);
    SelRect {
        x: area.x + area.w - tw,
        y: area.y + (area.h - th) / 2,
        w: tw,
        h: th,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base() -> Ui {
        Ui::from_dpi(96)
    }

    /// 所有部件在窗口内，且三段不重叠。
    #[test]
    fn parts_stay_inside_window() {
        for ui in [base(), Ui::from_dpi(192)] {
            for content in [0, 20, 100, 400, 5000] {
                let l = layout(ui, content);
                for (name, r) in [
                    ("pin", l.pin),
                    ("close", l.close),
                    ("body", l.body),
                    ("copy", l.copy),
                    ("auto_copy", l.auto_copy),
                ] {
                    assert!(r.x >= 0 && r.y >= 0, "{name} 越界：{r:?}");
                    assert!(r.x + r.w <= l.w, "{name} 超出右边界：{r:?}");
                    assert!(r.y + r.h <= l.h, "{name} 超出下边界：{r:?}");
                }
                assert!(l.pin.x + l.pin.w <= l.close.x, "图钉与关闭不能重叠");
                assert!(l.pin.y + l.pin.h <= l.body.y, "顶部图标在文本区之上");
                assert!(l.body.y >= l.frame.y && l.body.x >= l.frame.x, "文本区应在边框内");
                assert!(
                    l.body.x + l.body.w <= l.frame.x + l.frame.w && l.body.y + l.body.h <= l.frame.y + l.frame.h,
                    "文本区超出边框"
                );
                assert!(l.copy.y >= l.frame.y + l.frame.h, "复制按钮在边框之下");
                assert!(l.copy.x + l.copy.w <= l.auto_copy.x, "复制按钮与开关不能重叠");
                // 开关靠右：与右上角关闭图标右边线对齐
                assert_eq!(l.auto_copy.x + l.auto_copy.w, l.close.x + l.close.w, "开关应右对齐");
                // 轨道必须落在开关区域内
                let t = track_rect(ui, l.auto_copy);
                assert!(
                    t.x >= l.auto_copy.x && t.x + t.w <= l.auto_copy.x + l.auto_copy.w,
                    "轨道超出开关区域：{t:?}"
                );
            }
        }
    }

    /// 内容高度被夹在最小 / 最大之间：太短不塌、太长改滚动。
    #[test]
    fn body_height_is_clamped() {
        let ui = base();
        assert_eq!(layout(ui, 0).body.h, ui.px(MIN_BODY_BASE));
        assert_eq!(layout(ui, 10_000).body.h, ui.px(MAX_BODY_BASE));
        assert_eq!(layout(ui, 120).body.h, 120);
    }

    /// 上限可覆盖：屏幕装不下时按工作区高度收窄（OCR-7 不溢出屏幕）。
    #[test]
    fn cap_overrides_max_body_height() {
        let ui = base();
        let l = layout_capped(ui, 10_000, 120);
        assert_eq!(l.body.h, 120);
        assert!(l.h < layout(ui, 10_000).h, "收窄后总高度应变小");
        // 上限比最小高度还小时不能把文本区压没
        assert_eq!(layout_capped(ui, 10_000, 0).body.h, ui.px(MIN_BODY_BASE));
    }

    /// 高 DPI 下整体等比放大（CAP-5 / ADR 0016）。
    #[test]
    fn scales_with_dpi() {
        let a = layout(base(), 100);
        let b = layout(Ui::from_dpi(192), 100);
        assert_eq!(b.w, a.w * 2);
        assert_eq!(b.close.w, a.close.w * 2);
        assert_eq!(b.auto_copy.h, a.auto_copy.h * 2);
    }
}
