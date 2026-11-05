// SPDX-License-Identifier: GPL-3.0-only
//! 气泡绘制（OCR-8/OCR-9）：卡片外壳——底色、文本区边框、顶部图标行、底部动作行。
//!
//! 正文、选区、光标、滚动条都不在这里画：文本区是原生 `EDIT` 子控件（`ctl`），
//! 系统自己渲染。这里只负责它周围那圈外壳。
//!
//! 窗口是普通窗口（不是分层窗口），内容全不透明，圆角由 `SetWindowRgn` 裁掉。

use tiny_skia::{Pixmap, Transform};

use crate::overlay::geometry::SelRect;
use crate::overlay::shape::{paint_of, rounded_rect, rounded_rect_outline};
use crate::render::icon::{self, Icon};
use crate::render::text::{self, FontSpec};

use super::Bubble;
use super::interact::Hit;
use super::layout::{Layout, RADIUS_BASE, track_rect};

/// 正文字号（基准像素）：`>= 16`——从 13 起试，长段中文看着太小。
pub const BODY_SIZE_BASE: f32 = 16.0;
/// 「复制」按钮字号。
const LABEL_SIZE_BASE: f32 = 13.0;

/// 把气泡外壳画到像素图（尺寸必须等于 `layout.w/h`）。
pub fn render(pixmap: &mut Pixmap, b: &Bubble, l: &Layout) {
    let p = b.palette;
    let ui = b.ui;
    let r = ui.f(RADIUS_BASE);
    let all = SelRect {
        x: 0,
        y: 0,
        w: l.w,
        h: l.h,
    };
    // 底色铺满整幅，**不画圆角路径**：圆角由 `SetWindowRgn` 裁掉。
    // 若这里画圆角形状，路径外的像素是透明的，而 BitBlt 不认 alpha，
    // 会变成四个角上的一圈黑色抗锯齿边（实测现象：「有圆角，有黑边」）。
    let mut bg = p.bg;
    bg[3] = 255;
    if let Some(full) = tiny_skia::Rect::from_xywh(0.0, 0.0, l.w as f32, l.h as f32) {
        pixmap.fill_rect(full, &paint_of(bg), Transform::identity(), None);
    }

    // 文本区 1px 边框：常态淡色，文本区拿到焦点时换成强调色
    rounded_rect_outline(
        pixmap,
        l.frame,
        ui.f(6.0),
        if b.ctl.focused() { p.accent } else { p.divider },
    );

    // 顶部：图钉（点亮=钉住）+ 关闭，常驻但用次级色
    icon_badge(pixmap, b, l.pin, Icon::Pin, b.pinned, b.hover == Some(Hit::Pin));
    icon_badge(pixmap, b, l.close, Icon::Cross, false, b.hover == Some(Hit::Close));

    // 左下角「复制」按钮：点击即复制，并短暂显示「已复制」
    let label = FontSpec::new(ui.f(LABEL_SIZE_BASE));
    let copy_hot = b.hover == Some(Hit::Copy);
    if copy_hot || b.copied {
        rounded_rect(pixmap, l.copy, ui.f(6.0), p.accent_soft, None);
    }
    let caption = if b.copied { "已复制" } else { "复制" };
    let (tw, th) = text::measure(caption, &label);
    text::draw(
        pixmap,
        crate::editor::Point::new(
            l.copy.x as f32 + (l.copy.w as f32 - tw) / 2.0,
            l.copy.y as f32 + (l.copy.h as f32 - th) / 2.0,
        ),
        caption,
        &label,
        if copy_hot || b.copied { p.accent } else { p.text_dim },
        Transform::identity(),
    );

    // 右下角「自动复制」开关：文字 + 滑块
    let hovered = b.hover == Some(Hit::AutoCopy);
    let track = track_rect(ui, l.auto_copy);
    let (tw, th) = text::measure("自动复制", &label);
    text::draw(
        pixmap,
        crate::editor::Point::new(
            (track.x - ui.px(8.0)) as f32 - tw,
            l.auto_copy.y as f32 + (l.auto_copy.h as f32 - th) / 2.0,
        ),
        "自动复制",
        &label,
        if b.auto_copy || hovered { p.text } else { p.text_dim },
        Transform::identity(),
    );
    draw_switch(pixmap, b, track, b.auto_copy, hovered);

    // 外边框最后描一遍：内容压过圆角后边缘更清晰
    rounded_rect_outline(pixmap, all, r, p.border);
}

/// 图标底（悬停给一层实底）+ 图标本身。`on` 为真时用强调色（图钉点亮 / 复制完成）。
fn icon_badge(pixmap: &mut Pixmap, b: &Bubble, r: SelRect, icon: Icon, on: bool, hot: bool) {
    let p = b.palette;
    let ui = b.ui;
    if hot || on {
        rounded_rect(pixmap, r, ui.f(5.0), if on { p.accent_soft } else { p.row_hover }, None);
    }
    icon::draw(
        pixmap,
        icon,
        (r.x + r.w / 2) as f32,
        (r.y + r.h / 2) as f32,
        r.w as f32 * icon_scale(icon),
        if on || hot { p.accent } else { p.icon_dim },
    );
}

/// 滑动开关：胶囊轨道 + 白色滑钮（与设置面板同一视觉语言）。
fn draw_switch(pixmap: &mut Pixmap, b: &Bubble, track: SelRect, on: bool, hovered: bool) {
    let p = b.palette;
    let ui = b.ui;
    let fill = if on {
        p.toggle_on
    } else if hovered {
        p.divider
    } else {
        p.toggle_off
    };
    rounded_rect(pixmap, track, (track.h / 2) as f32, fill, None);
    let kd = track.h - ui.px(4.0);
    let kx = if on {
        track.x + track.w - kd - ui.px(2.0)
    } else {
        track.x + ui.px(2.0)
    };
    rounded_rect(
        pixmap,
        SelRect {
            x: kx,
            y: track.y + (track.h - kd) / 2,
            w: kd,
            h: kd,
        },
        (kd / 2) as f32,
        [255, 255, 255, 255],
        None,
    );
}

/// 图标在点击区里的实际边长系数。
///
/// 按墨水覆盖率对齐：`Pin` 14.8% 天生偏轻，`Cross` 25.4% 偏重
/// （实测见 `assets/icons/ui/README.md`）。用同一系数画，图钉会明显比关闭小一号。
///
/// 另：线框图标比同字号的中文视觉重量轻很多（用户反馈「图标比文字小」），
/// 所以整体往大里调——图标盒子 ≈ 正文的 1.3 倍。
fn icon_scale(icon: Icon) -> f32 {
    match icon {
        Icon::Pin => 0.90,
        Icon::Cross => 0.82,
        _ => 0.82,
    }
}
