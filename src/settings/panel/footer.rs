// SPDX-License-Identifier: GPL-3.0-only
//! 「关于」页底部退款提示条：一句小字说明「通过第三方购买请向卖方申请退款」。
//!
//! 位置固定贴窗口底缘、横贯内容区，上方一条分隔线与内容分开；**只画在关于页**
//! （其他页底部就是留白）。它不参与命中测试，也不进 `Layout::items`——纯提示、不可点，
//! 因此不影响任何行几何与点击逻辑。
//!
//! 高度受窗口总高约束：关于页内容已占 516 / 520，提示条只能走底部条，
//! 因此 `controls::H` 一并抬到 552（516 + `FOOTER_H` 24 + 底部内边距 8）。

use tiny_skia::{Pixmap, Rect, Transform};

use crate::editor::Point;
use crate::i18n::SettingsKey as K;
use crate::overlay::geometry::SelRect;
use crate::overlay::shape::paint_of;
use crate::render::text as dwrite;

use super::Panel;
use super::controls::{FOOTER_H, Layout, NAV_W, PAD, Section, W};

/// 提示条与窗口下缘之间的内边距。
const BOTTOM_PAD: i32 = 8;

/// 提示条矩形：底部对齐、横贯内容区（绘制与量宽测试共用，避免两处各算一遍）。
///
/// `height` 传 `layout.height` 而非常量 `H`：内容超高时窗口会跟着变高
/// （`layout.height = max(H, 内容高)`），提示条必须贴真实底缘，否则会压住内容。
pub(super) fn bar_rect(scale: f32, height: i32) -> SelRect {
    let w = (W as f32 * scale) as i32;
    let nav_w = (NAV_W as f32 * scale) as i32;
    let pad = (PAD as f32 * scale) as i32;
    let fh = (FOOTER_H as f32 * scale) as i32;
    let bp = (BOTTOM_PAD as f32 * scale) as i32;
    SelRect {
        x: nav_w + pad,
        y: height - fh - bp,
        w: w - nav_w - pad * 2,
        h: fh,
    }
}

/// 画提示条（只在关于页生效）。
pub(super) fn draw(pixmap: &mut Pixmap, panel: &Panel, layout: &Layout) {
    if panel.section != Section::About {
        return;
    }
    let p = panel.palette;
    let s = panel.scale;
    let bar = bar_rect(s, layout.height);
    // 分隔线：把提示条与上方内容分开，也让 4px 间隙看起来是刻意的
    if let Some(rect) = Rect::from_xywh(bar.x as f32, bar.y as f32, bar.w as f32, 1.0) {
        pixmap.fill_rect(rect, &paint_of(p.divider), Transform::identity(), None);
    }
    let size = 13.0 * s;
    let text = K::AboutRefundNote.text();
    // 文字在提示条内水平居中（用户要求：居中显示，不靠左）
    let (tw, _) = dwrite::measure(text, &dwrite::FontSpec::new(size));
    let x = bar.x as f32 + (bar.w as f32 - tw) / 2.0;
    let y = bar.y as f32 + (bar.h as f32 - size * 1.2) / 2.0;
    dwrite::draw(
        pixmap,
        Point::new(x, y),
        text,
        &dwrite::FontSpec::new(size),
        p.text_dim,
        Transform::identity(),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::i18n::Lang;
    use crate::settings::panel::controls::H;

    /// 提示条不与关于页内容重叠：内容底（516）必须落在提示条上方。
    #[test]
    fn bar_sits_below_about_content() {
        for s in [1.0f32, 1.25, 1.5, 2.0] {
            let height = (H as f32 * s) as i32;
            let bar = bar_rect(s, height);
            let content_bottom = (516.0 * s) as i32;
            assert!(
                bar.y >= content_bottom,
                "@{s}x 提示条压住了关于页内容：bar.y={} < 内容底 {content_bottom}",
                bar.y
            );
            assert!(bar.y + bar.h <= height, "@{s}x 提示条超出窗口下缘");
            assert!(bar.w > 0 && bar.h > 0, "@{s}x 提示条尺寸非法");
        }
    }

    /// 「我要反馈」的轻通知不能压在提示条上（两者都出现在关于页底部）。
    #[test]
    fn toast_stays_above_note() {
        let mut panel = Panel::for_test(Section::About, crate::theme::Theme::Dark);
        panel.toast = Some("x".to_string());
        let height = (H as f32 * panel.scale) as i32;
        let (toast, _) = crate::settings::panel::copyinfo::toast_rects(&panel, height).expect("有 toast 时应给出矩形");
        let bar = bar_rect(panel.scale, height);
        assert!(
            (toast.y + toast.h) <= bar.y,
            "toast 压住了退款提示条：toast 底 {} > 提示条顶 {}",
            toast.y + toast.h,
            bar.y
        );
    }

    /// 四门语言的提示文案都得装进提示条（放不下时 `dwrite::draw` 不会换行，会直接溢出）。
    #[test]
    fn every_language_note_fits_bar() {
        for lang in Lang::ALL {
            for s in [1.0f32, 1.25, 1.5, 2.0] {
                let text = crate::i18n::text(lang, crate::i18n::Key::Settings(K::AboutRefundNote));
                let spec = dwrite::FontSpec::new(13.0 * s);
                let (tw, _) = dwrite::measure(text, &spec);
                let bar = bar_rect(s, (H as f32 * s) as i32);
                assert!(
                    tw <= bar.w as f32,
                    "{lang:?} @{s}x：退款提示「{text}」量宽 {tw:.1} 超过提示条 {}",
                    bar.w
                );
            }
        }
    }
}
