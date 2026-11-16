// SPDX-License-Identifier: GPL-3.0-only
//! 快速保存成功的轻提示（OUT-2 / ADR 0041）：覆盖层内自绘一条非模态提示，
//! 显示刚写出的文件名，**点击它 = 在资源管理器里定位该文件**；到点自动收尾。
//!
//! 与取消二次确认（`cancel_hint`）同族：屏幕显示层、不进导出烘焙、不抢焦点。
//! 位置优先贴**选区下方**居中，下方塞不下翻到上方，最后钳回屏幕内。

use tiny_skia::{Pixmap, Transform};
use windows::Win32::UI::WindowsAndMessaging::{KillTimer, SetTimer};

use super::Overlay;
use super::geometry::{SelRect, expand_rect};
use super::shape::rounded_rect;
use super::ui::Ui;
use crate::editor::Point;
use crate::render::text::{self as dwrite, FontSpec};

/// 提示定时器 ID（**必须与其它用途全不同**，否则一个用途的定时器会被另一个抢走：
/// tip 2 / cancel_hint 3 / fade 4 / 候选框动画 5）。
/// ADR 0041 曾把它写成 5，结果吸附动画一触发就把覆盖层当成「提示到点」关掉了。
pub(super) const TIMER_ID: usize = 6;
/// 提示停留时长（毫秒）：到点即收起并结束本次截图。
pub(super) const SHOW_MS: u64 = 2000;

/// 深色不透明底 + 白字：在任意截图内容上都清晰，不随主题变化。
const BG: [u8; 4] = [32, 32, 32, 238];
const FG: [u8; 4] = [255, 255, 255, 255];
const BORDER: [u8; 4] = [26, 115, 232, 200];
const RADIUS_BASE: f32 = 8.0;
const PAD_X_BASE: f32 = 14.0;
const PAD_Y_BASE: f32 = 8.0;
const FONT_BASE: f32 = 14.0;
/// 与选区之间的间距（基准像素）。
const GAP_BASE: f32 = 8.0;
/// 离屏幕边缘的最小留白。
const MARGIN_BASE: f32 = 4.0;

/// 已弹出的提示：矩形 + 文案（到点由定时器收起）。
pub(super) struct Toast {
    pub(super) rect: SelRect,
    text: String,
}

impl Overlay {
    /// 弹出「已保存」提示并启动一次性定时器（到点由 `on_toast_timer` 收尾）。
    pub(super) fn show_saved_toast(&mut self, path: &std::path::Path) {
        self.clear_toast();
        let name = path
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| path.display().to_string());
        let text = format!("已保存 {name} · 点击定位文件");
        let (vw, vh) = (self.display.width() as i32, self.display.height() as i32);
        let anchor = self.selection.unwrap_or(SelRect {
            x: 0,
            y: 0,
            w: vw,
            h: 0,
        });
        let rect = toast_rect(&text, anchor, vw, vh, self.ui);
        self.quick.toast = Some(Toast { rect, text });
        // 同 ID 再次 SetTimer 会替换旧定时器（MSDN），无需先 Kill
        unsafe {
            let _ = SetTimer(Some(self.hwnd), TIMER_ID, SHOW_MS as u32, None);
        }
        self.repaint(expand_rect(rect, 2));
    }

    /// 提示到时：收起提示并结束本次截图（保存已成功，覆盖层没有继续留着的目的）。
    pub(super) fn on_toast_timer(&mut self) {
        unsafe {
            let _ = KillTimer(Some(self.hwnd), TIMER_ID);
        }
        self.cancelled = false;
        self.finish_quick(false);
    }

    /// 收起提示并重绘其所在区域。
    pub(super) fn clear_toast(&mut self) {
        unsafe {
            let _ = KillTimer(Some(self.hwnd), TIMER_ID);
        }
        if let Some(t) = self.quick.toast.take() {
            self.repaint(expand_rect(t.rect, 2));
        }
    }

    /// 提示是否仍可见（可见期间交互只认「收尾」）。
    pub(super) fn toast_visible(&self) -> bool {
        self.quick.toast.is_some()
    }

    /// 左键是否点在提示上（点提示 = 定位文件）。
    pub(super) fn toast_hit(&self, x: i32, y: i32) -> bool {
        matches!(&self.quick.toast, Some(t)
            if x >= t.rect.x && x < t.rect.x + t.rect.w && y >= t.rect.y && y < t.rect.y + t.rect.h)
    }
}

/// 提示矩形：选区下方居中 → 塞不下翻上方 → 钳回屏幕内（纯函数，便于单测）。
pub(super) fn toast_rect(text: &str, anchor: SelRect, vw: i32, vh: i32, ui: Ui) -> SelRect {
    let spec = FontSpec::new(ui.f(FONT_BASE));
    let (tw, th) = dwrite::measure(text, &spec);
    let w = (tw + ui.f(PAD_X_BASE) * 2.0).ceil() as i32;
    let h = (th + ui.f(PAD_Y_BASE) * 2.0).ceil() as i32;
    let gap = ui.px(GAP_BASE);
    let margin = ui.px(MARGIN_BASE);
    let below = anchor.y + anchor.h + gap;
    let above = anchor.y - gap - h;
    let y = if below + h + margin <= vh {
        below
    } else if above >= margin {
        above
    } else {
        // 上下都塞不下（选区几乎占满屏幕）：贴屏幕底部，宁可压住一点点内容
        (vh - h - margin).max(margin)
    };
    let cx = anchor.x + (anchor.w - w) / 2;
    SelRect {
        x: cx.clamp(margin, (vw - w - margin).max(margin)),
        y: y.clamp(margin, (vh - h - margin).max(margin)),
        w,
        h,
    }
}

/// 绘制提示（屏幕显示层，不进导出烘焙）。
pub(super) fn draw(pixmap: &mut Pixmap, toast: &Toast, ui: Ui) {
    rounded_rect(pixmap, toast.rect, ui.f(RADIUS_BASE), BG, Some(BORDER));
    let spec = FontSpec::new(ui.f(FONT_BASE));
    let (tw, th) = dwrite::measure(&toast.text, &spec);
    let x = toast.rect.x as f32 + (toast.rect.w as f32 - tw) / 2.0;
    let y = toast.rect.y as f32 + (toast.rect.h as f32 - th) / 2.0;
    dwrite::draw(pixmap, Point::new(x, y), &toast.text, &spec, FG, Transform::identity());
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sel(x: i32, y: i32, w: i32, h: i32) -> SelRect {
        SelRect { x, y, w, h }
    }

    /// 选区下方有空间：提示落在选区下方、水平居中。
    #[test]
    fn toast_sits_below_selection() {
        let ui = Ui::from_dpi(96);
        let (vw, vh) = (1920, 1080);
        let anchor = sel(400, 300, 600, 300);
        let r = toast_rect("已保存 a.png · 点击定位文件", anchor, vw, vh, ui);
        assert!(r.y >= anchor.y + anchor.h, "应在选区下方：{r:?}");
        let want_cx = anchor.x + anchor.w / 2;
        let got_cx = r.x + r.w / 2;
        assert!((got_cx - want_cx).abs() <= 2, "应水平居中：{r:?}");
        assert!(r.x >= 0 && r.x + r.w <= vw && r.y + r.h <= vh);
    }

    /// 选区贴屏幕底：下方塞不下 → 翻到选区上方。
    #[test]
    fn toast_flips_above_when_no_room_below() {
        let ui = Ui::from_dpi(96);
        let (vw, vh) = (1920, 1080);
        let anchor = sel(400, 900, 600, 170);
        let r = toast_rect("已保存 a.png · 点击定位文件", anchor, vw, vh, ui);
        assert!(r.y + r.h <= anchor.y, "应放在选区上方：{r:?} / {anchor:?}");
    }

    /// 选区几乎占满屏幕：上下都塞不下也要钳回屏幕内（不越界）。
    #[test]
    fn toast_clamps_into_screen() {
        let ui = Ui::from_dpi(96);
        let (vw, vh) = (1366, 768);
        for anchor in [
            sel(0, 0, 1366, 768),
            sel(0, 0, 40, 30),
            sel(1330, 740, 36, 28),
            sel(600, 300, 1, 1),
        ] {
            let r = toast_rect("已保存 a.png · 点击定位文件", anchor, vw, vh, ui);
            assert!(r.x >= 0 && r.x + r.w <= vw, "越出屏幕：{r:?}（选区 {anchor:?}）");
            assert!(r.y >= 0 && r.y + r.h <= vh, "越出屏幕：{r:?}（选区 {anchor:?}）");
        }
    }

    /// 高 DPI 下提示同步放大（CAP-5）。
    #[test]
    fn toast_scales_with_dpi() {
        let anchor = sel(100, 100, 800, 600);
        let norm = toast_rect("已保存 a.png · 点击定位文件", anchor, 1920, 1080, Ui::from_dpi(96));
        let hidpi = toast_rect("已保存 a.png · 点击定位文件", anchor, 3840, 2160, Ui::from_dpi(192));
        assert!(hidpi.h > norm.h && hidpi.w > norm.w, "高 DPI 应放大");
    }
}
