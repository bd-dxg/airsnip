// SPDX-License-Identifier: GPL-3.0-only
//! 长截图状态条按钮的悬浮提示（交互补全）：停留 400ms 后在外侧弹出「名称 · 快捷键」
//! 小气泡，与覆盖层工具栏（EDT-10 / ADR 0020）用同一套几何与绘制（`overlay::toolbar::tip`），
//! 不另写一份观感。悬停底色在 `bar::render` 里画。
//!
//! 独立分层窗口且带 `WS_EX_TRANSPARENT`：气泡若吃掉鼠标消息，状态条就收不到
//! `WM_MOUSELEAVE`，悬停会在「显示 → 隐藏 → 显示」之间闪。气泡按实际尺寸逐次
//! `UpdateLayeredWindow` 提交，不抢焦点、不常驻定时器。

use windows::Win32::Foundation::{COLORREF, HINSTANCE, HWND, LPARAM, LRESULT, POINT, SIZE, WPARAM};
use windows::Win32::Graphics::Gdi::{AC_SRC_ALPHA, AC_SRC_OVER, BLENDFUNCTION, DeleteDC, DeleteObject, HBITMAP, HDC};
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::w;

use super::bar::{Bar, Button};
use crate::overlay::geometry::SelRect;
use crate::overlay::toolbar::tip::{Bubble, bubble_rect, draw};
use crate::overlay::ui::Ui;
use crate::theme::Palette;

/// 悬停延迟（毫秒），与覆盖层工具栏一致（ADR 0020）。
pub(super) const DELAY_MS: u32 = 400;
/// 一次性定时器 ID（同一窗口内唯一：1 = 抓帧拍，见 `window::TIMER_ID`）。
pub(super) const TIMER_ID: usize = 2;

/// 气泡窗口与它的上屏缓冲。
pub(super) struct Tip {
    hwnd: HWND,
    mem_dc: HDC,
    bmp: HBITMAP,
    bits: *mut u8,
    /// 缓冲区像素尺寸（按最长文案预留；实际气泡更小时只提交左上角一块）。
    max_w: i32,
    max_h: i32,
    visible: bool,
}

/// 气泡窗口的窗口过程：不处理任何消息，全部交给系统默认（不抢焦点、不吃鼠标）。
unsafe extern "system" fn def_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
}

impl Tip {
    /// 创建（隐藏）气泡窗口。失败返回 `None`：提示是锦上添花，不能拖垮长截图。
    pub(super) fn create(hinstance: HINSTANCE, ui: Ui) -> Option<Tip> {
        let (max_w, max_h) = max_bubble_size(ui)?;
        unsafe {
            let wc = WNDCLASSW {
                lpfnWndProc: Some(def_proc),
                hInstance: hinstance,
                lpszClassName: w!("airsnip.longshot.tip"),
                ..Default::default()
            };
            let _ = RegisterClassW(&wc);
            let hwnd = CreateWindowExW(
                // WS_EX_TRANSPARENT：鼠标穿透，悬停不会因为气泡盖住而丢失
                WS_EX_LAYERED | WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE | WS_EX_TRANSPARENT,
                w!("airsnip.longshot.tip"),
                w!(""),
                WS_POPUP,
                0,
                0,
                1,
                1,
                None,
                None,
                Some(hinstance),
                None,
            )
            .ok()?;
            let (mem_dc, bmp, bits) = crate::overlay::surface::create_dib_surface(max_w as u32, max_h as u32).ok()?;
            Some(Self {
                hwnd,
                mem_dc,
                bmp,
                bits,
                max_w,
                max_h,
                visible: false,
            })
        }
    }

    /// 在状态条外侧弹出气泡。`anchor` 为按钮方框、`bar` 为整条状态条，均为**屏幕坐标**。
    pub(super) fn show(&mut self, button: Button, anchor: SelRect, bar: &Bar, palette: &Palette) {
        let ui = bar.ui;
        // 覆盖层那套几何按「虚拟屏左上角为原点」计算，这里换算过去再换回屏幕坐标
        let (vx, vy, vw, vh) = virtual_screen();
        let shift = |r: SelRect| SelRect {
            x: r.x - vx,
            y: r.y - vy,
            ..r
        };
        let text = button.tip_text();
        let r = bubble_rect(shift(anchor), shift(bar.rect), &text, vw, vh, ui);
        if r.w <= 0 || r.h <= 0 || r.w > self.max_w || r.h > self.max_h {
            return;
        }
        let Some(mut pixmap) = tiny_skia::Pixmap::new(r.w as u32, r.h as u32) else {
            return;
        };
        // 气泡画在局部坐标系（0,0）上，位置由 UpdateLayeredWindow 决定
        let local = SelRect {
            x: 0,
            y: 0,
            w: r.w,
            h: r.h,
        };
        draw(&mut pixmap, &Bubble::new(local, text), palette, ui);
        // RGBA（premultiplied）→ 逐行拷进内存 DIB（行距按缓冲区宽度）
        let src = super::present::premul_bgra(pixmap.data());
        unsafe {
            for y in 0..r.h as usize {
                std::ptr::copy_nonoverlapping(
                    src.as_ptr().add(y * r.w as usize * 4),
                    self.bits.add(y * self.max_w as usize * 4),
                    r.w as usize * 4,
                );
            }
            let dst = POINT {
                x: r.x + vx,
                y: r.y + vy,
            };
            let size = SIZE { cx: r.w, cy: r.h };
            let src_pt = POINT { x: 0, y: 0 };
            let blend = BLENDFUNCTION {
                BlendOp: AC_SRC_OVER as u8,
                BlendFlags: 0,
                SourceConstantAlpha: 255,
                AlphaFormat: AC_SRC_ALPHA as u8,
            };
            let ok = UpdateLayeredWindow(
                self.hwnd,
                None,
                Some(&dst),
                Some(&size),
                Some(self.mem_dc),
                Some(&src_pt),
                COLORREF(0),
                Some(&blend),
                ULW_ALPHA,
            );
            if ok.is_ok() {
                let _ = ShowWindow(self.hwnd, SW_SHOWNOACTIVATE);
                self.visible = true;
            }
        }
    }

    /// 收起气泡（悬停目标变化 / 鼠标移出 / 按下鼠标）。
    pub(super) fn hide(&mut self) {
        if self.visible {
            unsafe {
                let _ = ShowWindow(self.hwnd, SW_HIDE);
            }
            self.visible = false;
        }
    }
}

impl Drop for Tip {
    fn drop(&mut self) {
        unsafe {
            let _ = DestroyWindow(self.hwnd);
            if !self.bmp.0.is_null() {
                let _ = DeleteObject(self.bmp.into());
            }
            if !self.mem_dc.0.is_null() {
                let _ = DeleteDC(self.mem_dc);
            }
        }
    }
}

/// 虚拟屏（所有显示器的并集）：气泡几何与钳位都用它。
fn virtual_screen() -> (i32, i32, i32, i32) {
    unsafe {
        (
            GetSystemMetrics(SM_XVIRTUALSCREEN),
            GetSystemMetrics(SM_YVIRTUALSCREEN),
            GetSystemMetrics(SM_CXVIRTUALSCREEN).max(1),
            GetSystemMetrics(SM_CYVIRTUALSCREEN).max(1),
        )
    }
}

/// 缓冲区尺寸：取四个按钮文案里最大的一个气泡（量宽走 DirectWrite，与绘制同源）。
fn max_bubble_size(ui: Ui) -> Option<(i32, i32)> {
    let dummy = SelRect { x: 0, y: 0, w: 1, h: 1 };
    let mut size = (0, 0);
    for button in Button::ALL {
        let r = bubble_rect(dummy, dummy, &button.tip_text(), 100_000, 100_000, ui);
        size = (size.0.max(r.w), size.1.max(r.h));
    }
    (size.0 > 0 && size.1 > 0).then_some(size)
}
