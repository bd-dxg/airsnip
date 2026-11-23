// SPDX-License-Identifier: GPL-3.0-only
//! 关于页「我要反馈」（SYS-8）：复制系统信息 → 面板底部弹一条通知 → 用户**自己点**
//! 「打开反馈页」才开浏览器；不点就自动消失（只保留剪贴板里的信息）。
//!
//! 不自动打开页面：用户很可能已经从网页进入 issue（引导就写在模板里），再自动跳一次
//! 浏览器只会多开一个标签。复制内容只服务「日志与补充信息」字段。
//!
//! 用一次性 `SetTimer` 而不是轮询：到点即 `KillTimer`，待机 CPU 仍是 0%。

use std::time::Duration;

use tiny_skia::{Pixmap, Transform};
use windows::Win32::Foundation::HWND;
use windows::Win32::UI::WindowsAndMessaging::{KillTimer, SetTimer};

use crate::editor::Point;
use crate::i18n::SettingsKey as K;
use crate::overlay::geometry::{SelRect, in_rect};
use crate::overlay::shape::rounded_rect;
use crate::render::text as dwrite;

use super::Panel;

/// 通知停留时长：够把鼠标移过来点按钮，又不至于挡住面板（改这里即可调整）。
pub(super) const HOLD: Duration = Duration::from_secs(4);
/// 面板定时器 ID（`WM_TIMER` 里按它识别；面板目前没有其它定时器）。
pub(super) const TIMER_ID: usize = 1;

/// 通知文案在 `i18n`（键 `AboutToast*`）；按钮文案取当前界面语言。
fn btn_text() -> &'static str {
    K::AboutToastOpen.text()
}

impl Panel {
    /// 点「我要反馈」：复制系统信息并弹通知；打开网页由用户点通知里的按钮决定。
    pub(super) fn start_feedback(&mut self) {
        let text = match crate::diagnose::copy_report(&self.config) {
            Ok(()) => K::AboutToastCopied.text(),
            Err(_) => K::AboutToastFailed.text(),
        };
        self.toast = Some(text.to_string());
        unsafe {
            SetTimer(Some(self.hwnd), TIMER_ID, HOLD.as_millis() as u32, None);
        }
    }
}

/// 定时器到点：收起通知（不打开页面），重绘一次。
pub(super) fn on_timer(hwnd: HWND) {
    unsafe {
        let _ = KillTimer(Some(hwnd), TIMER_ID);
    }
    if let Some(panel) = super::window::app_from(hwnd)
        && panel.toast.take().is_some()
    {
        super::window::repaint_full(hwnd, panel);
    }
}

/// 通知条与其中「打开反馈页」按钮的矩形（绘制 / 命中 / 悬停共用一份几何）。
pub(super) fn toast_rects(panel: &Panel, height: i32) -> Option<(SelRect, SelRect)> {
    let text = panel.toast.as_deref()?;
    let s = panel.scale;
    let (tw, _) = dwrite::measure(text, &dwrite::FontSpec::new(14.0 * s));
    let (bw, _) = dwrite::measure(btn_text(), &dwrite::FontSpec::new(13.0 * s));
    let pad = 14.0 * s;
    let gap = 14.0 * s;
    let btn_pad = 12.0 * s;
    let bh = 28.0 * s;
    let btn_w = bw + btn_pad * 2.0;
    let w = pad * 2.0 + tw + gap + btn_w;
    let h = bh + 12.0 * s;
    let x = (panel.content_w() as f32 - w) / 2.0;
    let y = height as f32 - h - 12.0 * s;
    let toast = SelRect {
        x: x as i32,
        y: y as i32,
        w: w as i32,
        h: h as i32,
    };
    let btn = SelRect {
        x: (x + w - pad - btn_w) as i32,
        y: (y + (h - bh) / 2.0) as i32,
        w: btn_w as i32,
        h: bh as i32,
    };
    Some((toast, btn))
}

/// 通知是否在鼠标下（命中时不上报行悬停，避免通知下面的行跟着高亮）。
pub(super) fn is_over(panel: &Panel, x: i32, y: i32) -> bool {
    let height = panel.layout().height;
    toast_rects(panel, height).is_some_and(|(toast, _)| in_rect(toast, x, y))
}

/// 点击通知：按钮区打开反馈页，其余区域只吞掉事件。返回是否已处理。
pub(super) fn on_click(panel: &mut Panel, x: i32, y: i32) -> bool {
    let height = panel.layout().height;
    let Some((toast, btn)) = toast_rects(panel, height) else {
        return false;
    };
    if !in_rect(toast, x, y) {
        return false;
    }
    if in_rect(btn, x, y) {
        crate::app::open::url(crate::app::open::NEW_ISSUE_URL);
    }
    panel.toast = None;
    true
}

/// 通知条绘制：圆角底 + 文案 + 「打开反馈页」按钮（按钮用强调实底，一眼看得出能点）。
pub(super) fn draw_toast(pixmap: &mut Pixmap, panel: &Panel, height: i32) {
    let Some((toast, btn)) = toast_rects(panel, height) else {
        return;
    };
    let p = panel.palette;
    let s = panel.scale;
    rounded_rect(pixmap, toast, (8.0 * s) as f32, p.accent_soft, Some(p.accent));

    let text = panel.toast.as_deref().unwrap_or_default();
    let size = 14.0 * s;
    let tx = toast.x as f32 + 14.0 * s;
    let ty = toast.y as f32 + (toast.h as f32 - size * 1.2) / 2.0;
    dwrite::draw(
        pixmap,
        Point::new(tx, ty),
        text,
        &dwrite::FontSpec::new(size),
        p.accent,
        Transform::identity(),
    );

    rounded_rect(pixmap, btn, (6.0 * s) as f32, p.accent, None);
    let bsize = 13.0 * s;
    let (bw, _) = dwrite::measure(btn_text(), &dwrite::FontSpec::new(bsize));
    let bx = btn.x as f32 + (btn.w as f32 - bw) / 2.0;
    let by = btn.y as f32 + (btn.h as f32 - bsize * 1.2) / 2.0;
    dwrite::draw(
        pixmap,
        Point::new(bx, by),
        btn_text(),
        &dwrite::FontSpec::new(bsize),
        [255, 255, 255, 255],
        Transform::identity(),
    );
}
