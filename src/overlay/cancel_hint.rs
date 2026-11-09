// SPDX-License-Identifier: GPL-3.0-only
//! 取消的二次确认（OUT-3 / ADR 0027 / issue #120）：有未保存标注且开启防护时，
//! `Esc` / 右键第一次只「待命」并弹出非模态提示，同一键在时间窗内再按一次才退出。
//!
//! 用自绘提示替代系统确认框：不打断操作，与「防误触丢标注」的目的等价
//! （移动端「连按两次返回退出」同款交互）。提示用琥珀色提醒底，保证在任意截图内容上都醒目；
//! 位置优先贴**选区内部右上角**，选区过小时退到**选区上方**（上方无空间则**下方**）。
//! 几何 / 绘制 / 计时见本模块，取消分层见 `annotate`。

use std::time::{Duration, Instant};

use tiny_skia::{Pixmap, Transform};
use windows::Win32::UI::WindowsAndMessaging::{KillTimer, SetTimer};

use super::Overlay;
use super::geometry::{SelRect, expand_rect};
use super::shape::rounded_rect;
use super::ui::Ui;
use crate::editor::Point;
use crate::render::text::{self as dwrite, FontSpec};

/// 二次确认时间窗（毫秒）：待命后须在此窗口内按同一键才退出。
pub(super) const WINDOW_MS: u64 = 2000;
/// 渐隐时长（毫秒）：窗口末尾这段内提示线性淡出。
pub(super) const FADE_MS: u64 = 500;
/// 保持阶段时长（毫秒）：先静止显示这么久，到点后转入渐隐。
const HOLD_MS: u64 = WINDOW_MS - FADE_MS;
/// 渐隐阶段的刷新周期（毫秒）。
const TICK_MS: u64 = 30;
/// 提示计时的一次性 / 周期定时器 ID（与 tip 的 2 区分）。
pub(super) const TIMER_ID: usize = 3;
/// 右键的键标识（右键没有 VK，取 `VK_RBUTTON` 的值，仅用于区分退出键）。
pub(super) const KEY_RBUTTON: u32 = 0x02;
/// Esc 的键标识。
pub(super) const KEY_ESCAPE: u32 = 0x1B;

/// 提醒底色（琥珀）与文字色：不透明、在任意截图内容上都醒目，不随主题变化。
const ALERT_BG: [u8; 4] = [255, 179, 0, 255];
const ALERT_FG: [u8; 4] = [40, 40, 40, 255];
const ALERT_BORDER: [u8; 4] = [120, 80, 0, 170];
/// 贴在选区内部时距选区边缘的内边距（基准像素）。
const INSET_BASE: f32 = 8.0;
/// 退到选区外（上 / 下）时与选区边缘的间距（基准像素）。
const GAP_BASE: f32 = 8.0;
/// 提示水平内边距。
const PAD_X_BASE: f32 = 14.0;
/// 提示垂直内边距。
const PAD_Y_BASE: f32 = 8.0;
/// 提示圆角半径。
const RADIUS_BASE: f32 = 8.0;
/// 提示字号。
const FONT_BASE: f32 = 14.0;
/// 离屏幕边缘的最小留白。
const MARGIN_BASE: f32 = 4.0;
/// 贴在选区内部时，提示最多占选区可用高度的比例：小选区里提示若几乎占满高度，
/// 会盖掉大半内容（issue #120 测试反馈），此时改为退到选区外。
const INTERIOR_MAX_HEIGHT_RATIO: f32 = 0.5;

/// 已弹出的提示：矩形 + 文字 + 起始时刻（渐隐用）+ 是否已进入渐隐阶段。
pub(super) struct CancelHint {
    pub(super) rect: SelRect,
    text: String,
    start: Instant,
    fading: bool,
}

impl Overlay {
    /// 记录一次退出意图（`Esc` / 右键）。返回 `true` 表示同键在时间窗内二次触发，应真正退出。
    pub(super) fn arm_cancel(&mut self, key: u32) -> bool {
        let now = Instant::now();
        if is_second_press(self.cancel_arm, key, now) {
            // 马上关窗，无需重绘提示
            self.cancel_hint = None;
            return true;
        }
        self.cancel_arm = Some((key, now));
        self.show_cancel_hint(key, now);
        false
    }

    /// 解除退出意图并收起提示（任何其它输入都应调用，避免残留一次「待命」）。
    pub(super) fn clear_cancel_arm(&mut self) {
        self.cancel_arm = None;
        self.clear_cancel_hint();
    }

    /// 提示计时：保持阶段到点后转入周期渐隐，窗口结束收起提示。
    pub(super) fn on_cancel_timer(&mut self) {
        let Some((start, fading, rect)) = self.cancel_hint.as_ref().map(|h| (h.start, h.fading, h.rect)) else {
            unsafe {
                let _ = KillTimer(Some(self.hwnd), TIMER_ID);
            }
            return;
        };
        if start.elapsed() >= Duration::from_millis(WINDOW_MS) {
            unsafe {
                let _ = KillTimer(Some(self.hwnd), TIMER_ID);
            }
            self.cancel_arm = None;
            self.clear_cancel_hint();
            return;
        }
        if !fading {
            // 保持阶段的一次性计时到点：转入周期刷新，开始末尾渐隐（本次不重绘）
            if let Some(h) = self.cancel_hint.as_mut() {
                h.fading = true;
            }
            unsafe {
                let _ = SetTimer(Some(self.hwnd), TIMER_ID, TICK_MS as u32, None);
            }
            return;
        }
        self.repaint(expand_rect(rect, 2));
    }

    /// 弹出提示并启动保持阶段的一次性定时器。
    fn show_cancel_hint(&mut self, key: u32, now: Instant) {
        // 换退出键重新待命时，先清掉上一条提示：文案 / 位置 / 宽度可能不同，
        // 只重绘新矩形会让旧提示（及其边框）残留在屏幕上。
        if let Some(old) = self.cancel_hint.take() {
            self.repaint(expand_rect(old.rect, 2));
        }
        let text = hint_text(key, self.locked_selection);
        let (vw, vh) = (self.display.width() as i32, self.display.height() as i32);
        // 进入二次确认时必然有选区（有标注即有选区）；兜底锚到屏幕顶部，仍落在右上角
        let anchor = self.selection.unwrap_or(SelRect {
            x: 0,
            y: 0,
            w: vw,
            h: 0,
        });
        let rect = hint_rect(&text, anchor, vw, vh, self.ui);
        self.cancel_hint = Some(CancelHint {
            rect,
            text,
            start: now,
            fading: false,
        });
        // 同 ID 再次 SetTimer 会替换旧定时器（MSDN），无需先 Kill
        unsafe {
            let _ = SetTimer(Some(self.hwnd), TIMER_ID, HOLD_MS as u32, None);
        }
        self.repaint(expand_rect(rect, 2));
    }

    /// 收起提示并重绘其所在区域。
    fn clear_cancel_hint(&mut self) {
        unsafe {
            let _ = KillTimer(Some(self.hwnd), TIMER_ID);
        }
        if let Some(h) = self.cancel_hint.take() {
            self.repaint(expand_rect(h.rect, 2));
        }
    }
}

/// 是否构成二次触发：同键且在时间窗内（纯函数，便于单测）。
pub(super) fn is_second_press(prev: Option<(u32, Instant)>, key: u32, now: Instant) -> bool {
    matches!(prev, Some((k, t)) if k == key && now.duration_since(t) <= Duration::from_millis(WINDOW_MS))
}

/// 渐隐透明度：前 `HOLD_MS` 保持 1.0，末尾 `FADE_MS` 线性降到 0（纯函数，便于单测）。
pub(super) fn fade_alpha(elapsed_ms: f32) -> f32 {
    let fade_start = HOLD_MS as f32;
    if elapsed_ms <= fade_start {
        1.0
    } else {
        (1.0 - (elapsed_ms - fade_start) / FADE_MS as f32).clamp(0.0, 1.0)
    }
}

/// 提示文案：随退出键与「截图 / 贴图编辑」模式变化。
pub(super) fn hint_text(key: u32, editing: bool) -> String {
    let k = if key == KEY_RBUTTON { "右键" } else { "Esc" };
    if editing {
        format!("再按一次 {k} 放弃标注并退出编辑")
    } else {
        format!("再按一次 {k} 放弃标注并退出截图")
    }
}

/// 提示几何：优先贴在**选区内部右上角**；选区塞不下整个提示时退到**选区上方**，
/// 上方空间不足（选区贴屏幕顶）再退到**下方**；最后水平钳回屏幕内。
pub(super) fn hint_rect(text: &str, anchor: SelRect, vw: i32, vh: i32, ui: Ui) -> SelRect {
    let spec = FontSpec::new(ui.f(FONT_BASE));
    let (tw, th) = dwrite::measure(text, &spec);
    let (pad_x, pad_y) = (ui.f(PAD_X_BASE), ui.f(PAD_Y_BASE));
    let w = (tw + pad_x * 2.0).ceil() as i32;
    let h = (th + pad_y * 2.0).ceil() as i32;
    let inset = ui.px(INSET_BASE);
    let gap = ui.px(GAP_BASE);
    let margin = ui.px(MARGIN_BASE);
    let clamp_x = |x: i32| x.clamp(margin, (vw - w - margin).max(margin));
    let clamp_y = |y: i32| y.clamp(margin, (vh - h - margin).max(margin));

    // 1) 选区内部右上角：宽塞得下，且提示高度不超过可用高度的一半
    //    （「刚好够高」也算挤：小选区里提示会盖掉大半内容，issue #120 测试反馈）
    let room_h = (anchor.h - inset * 2).max(0) as f32;
    if w + inset <= anchor.w && (h as f32) <= room_h * INTERIOR_MAX_HEIGHT_RATIO {
        return SelRect {
            x: clamp_x(anchor.x + anchor.w - inset - w),
            y: clamp_y(anchor.y + inset),
            w,
            h,
        };
    }
    // 2) 选区上方；上方空间不足（贴屏幕顶）则下方
    let above = anchor.y - gap - h;
    let y = if above >= margin {
        above
    } else {
        anchor.y + anchor.h + gap
    };
    SelRect {
        x: clamp_x(anchor.x + anchor.w - w),
        y: clamp_y(y),
        w,
        h,
    }
}

/// 绘制提示（屏幕显示层，不进导出烘焙）；末尾按 `fade_alpha` 淡出。
pub(super) fn draw(pixmap: &mut Pixmap, hint: &CancelHint, ui: Ui) {
    let a = fade_alpha(hint.start.elapsed().as_secs_f32() * 1000.0);
    let scale = |v: u8, a: f32| (v as f32 * a).round() as u8;
    let bg = [ALERT_BG[0], ALERT_BG[1], ALERT_BG[2], scale(ALERT_BG[3], a)];
    let fg = [ALERT_FG[0], ALERT_FG[1], ALERT_FG[2], scale(ALERT_FG[3], a)];
    let border = [
        ALERT_BORDER[0],
        ALERT_BORDER[1],
        ALERT_BORDER[2],
        scale(ALERT_BORDER[3], a),
    ];
    rounded_rect(pixmap, hint.rect, ui.f(RADIUS_BASE), bg, Some(border));
    let spec = FontSpec::new(ui.f(FONT_BASE));
    let (tw, th) = dwrite::measure(&hint.text, &spec);
    let x = hint.rect.x as f32 + (hint.rect.w as f32 - tw) / 2.0;
    let y = hint.rect.y as f32 + (hint.rect.h as f32 - th) / 2.0;
    dwrite::draw(pixmap, Point::new(x, y), &hint.text, &spec, fg, Transform::identity());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn second_press_needs_same_key_within_window() {
        let t0 = Instant::now();
        assert!(!is_second_press(None, KEY_ESCAPE, t0));
        // 同键、窗口内 → 命中
        assert!(is_second_press(
            Some((KEY_ESCAPE, t0)),
            KEY_ESCAPE,
            t0 + Duration::from_millis(WINDOW_MS)
        ));
        // 换键不算（右键 / Esc 各自独立）
        assert!(!is_second_press(
            Some((KEY_RBUTTON, t0)),
            KEY_ESCAPE,
            t0 + Duration::from_millis(100)
        ));
        // 超出窗口 → 重新待命
        assert!(!is_second_press(
            Some((KEY_ESCAPE, t0)),
            KEY_ESCAPE,
            t0 + Duration::from_millis(WINDOW_MS + 1)
        ));
    }

    /// 渐隐：前段满不透明，末尾线性降到 0，越界夹紧。
    #[test]
    fn fade_alpha_holds_then_fades_to_zero() {
        assert_eq!(fade_alpha(0.0), 1.0);
        assert_eq!(fade_alpha(HOLD_MS as f32), 1.0);
        // 渐隐中点约 0.5
        let mid = fade_alpha((HOLD_MS + FADE_MS / 2) as f32);
        assert!((mid - 0.5).abs() < 0.05, "渐隐中点应约 0.5，实际 {mid}");
        assert_eq!(fade_alpha(WINDOW_MS as f32), 0.0);
        // 越界不越 0 / 1
        assert_eq!(fade_alpha(WINDOW_MS as f32 + 999.0), 0.0);
        assert_eq!(fade_alpha(-5.0), 1.0);
    }

    #[test]
    fn hint_text_mentions_key_and_mode() {
        assert!(hint_text(KEY_RBUTTON, false).contains("右键"));
        assert!(hint_text(KEY_ESCAPE, false).contains("Esc"));
        assert!(hint_text(KEY_ESCAPE, false).contains("退出截图"));
        assert!(hint_text(KEY_ESCAPE, true).contains("退出编辑"));
    }

    /// 选区足够大：提示贴在选区内部右上角（上内边距 + 右内边距），并完整落在屏幕内。
    #[test]
    fn hint_rect_sits_in_selection_top_right() {
        let ui = Ui::from_dpi(96);
        let (vw, vh) = (1920, 1080);
        let sel = SelRect {
            x: 400,
            y: 300,
            w: 900,
            h: 600,
        };
        let r = hint_rect("再按一次右键放弃标注并退出截图", sel, vw, vh, ui);
        let inset = ui.px(INSET_BASE);
        assert_eq!(r.x + r.w, sel.x + sel.w - inset, "应贴选区右内边：{r:?}");
        assert_eq!(r.y, sel.y + inset, "应贴选区上内边：{r:?}");
        assert!(r.x >= 0 && r.x + r.w <= vw && r.y >= 0 && r.y + r.h <= vh);
    }

    /// 选区太窄 / 太矮：提示退到选区上方。
    #[test]
    fn hint_rect_falls_above_small_selection() {
        let ui = Ui::from_dpi(96);
        let (vw, vh) = (1920, 1080);
        let sel = SelRect {
            x: 400,
            y: 500,
            w: 120,
            h: 60,
        };
        let r = hint_rect("再按一次右键放弃标注并退出截图", sel, vw, vh, ui);
        assert!(r.y + r.h <= sel.y, "应放在选区上方：{r:?} / {sel:?}");
        assert!(r.x >= 0 && r.x + r.w <= vw && r.y >= 0 && r.y + r.h <= vh);
    }

    /// 选区贴屏幕顶且过小：上方无空间 → 退到选区下方。
    #[test]
    fn hint_rect_falls_below_when_no_room_above() {
        let ui = Ui::from_dpi(96);
        let (vw, vh) = (1920, 1080);
        let sel = SelRect {
            x: 400,
            y: 0,
            w: 120,
            h: 60,
        };
        let r = hint_rect("再按一次 Esc 放弃标注并退出截图", sel, vw, vh, ui);
        assert!(r.y >= sel.y + sel.h, "上方无空间时应放下方：{r:?} / {sel:?}");
        assert!(r.x >= 0 && r.x + r.w <= vw && r.y >= 0 && r.y + r.h <= vh);
    }

    /// 选区高度只是「微微」大于提示：仍退到选区外，避免提示盖掉小选区大半内容（issue #120 测试反馈）。
    #[test]
    fn hint_rect_falls_out_when_selection_barely_fits_hint() {
        let ui = Ui::from_dpi(96);
        let (vw, vh) = (1920, 1080);
        let text = "再按一次 Esc 放弃标注并退出截图";
        let spec = FontSpec::new(ui.f(FONT_BASE));
        let (tw, th) = dwrite::measure(text, &spec);
        let hint_w = (tw + ui.f(PAD_X_BASE) * 2.0).ceil() as i32;
        let hint_h = (th + ui.f(PAD_Y_BASE) * 2.0).ceil() as i32;
        // 宽度宽裕、高度只比提示多几像素（用户反馈的「微微大于」场景）
        let sel = SelRect {
            x: 400,
            y: 400,
            w: hint_w + 200,
            h: hint_h + 6,
        };
        let r = hint_rect(text, sel, vw, vh, ui);
        assert!(
            r.y + r.h <= sel.y || r.y >= sel.y + sel.h,
            "提示不应落在选区内：{r:?} / {sel:?}"
        );
        assert!(r.x >= 0 && r.x + r.w <= vw && r.y >= 0 && r.y + r.h <= vh);
    }

    /// 选区贴边时提示仍钳回屏幕内、完整可见。
    #[test]
    fn hint_rect_clamps_into_screen() {
        let ui = Ui::from_dpi(96);
        let (vw, vh) = (1366, 768);
        for sel in [
            SelRect {
                x: 0,
                y: 0,
                w: 60,
                h: 40,
            },
            SelRect {
                x: 1300,
                y: 740,
                w: 60,
                h: 20,
            },
            SelRect { x: 0, y: 0, w: 1, h: 1 },
            SelRect {
                x: 660,
                y: 380,
                w: 6,
                h: 6,
            },
        ] {
            let r = hint_rect("再按一次 Esc 放弃标注并退出截图", sel, vw, vh, ui);
            assert!(r.x >= 0 && r.x + r.w <= vw, "越出屏幕：{r:?}（选区 {sel:?}）");
            assert!(r.y >= 0 && r.y + r.h <= vh, "越出屏幕：{r:?}（选区 {sel:?}）");
        }
    }

    /// 高 DPI 下提示同步放大（CAP-5）。
    #[test]
    fn hint_rect_scales_with_dpi() {
        let sel = SelRect {
            x: 100,
            y: 100,
            w: 1200,
            h: 800,
        };
        let norm = hint_rect("再按一次 Esc 放弃标注并退出截图", sel, 1920, 1080, Ui::from_dpi(96));
        let hidpi = hint_rect("再按一次 Esc 放弃标注并退出截图", sel, 3840, 2160, Ui::from_dpi(192));
        assert!(hidpi.h > norm.h && hidpi.w > norm.w, "高 DPI 应放大");
    }
}
