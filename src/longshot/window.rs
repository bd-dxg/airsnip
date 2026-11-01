// SPDX-License-Identifier: GPL-3.0-only
//! 长截图执行窗口（LNG-1/3/6）：置顶**分层**状态条，风格与截图工具栏一致
//! （tiny-skia 绘制，见 `bar`）。键位：Enter 完成、Esc 取消、Ctrl+Z 撤销；
//! 也可用鼠标点条上的按钮。定时器每 ~90ms 驱动 `Session::tick` 抓帧拼接。
//!
//! 用户自行在目标页面上滚动鼠标滚轮（不做自动滚动）；执行窗口用 `WS_EX_NOACTIVATE`
//! 不抢焦点，键盘走注册到本窗口的全局热键。

use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::Graphics::Gdi::{DeleteDC, DeleteObject, HBITMAP, HDC};
use windows::Win32::UI::Input::Ime::{HIMC, ImmAssociateContext};
use windows::Win32::UI::Input::KeyboardAndMouse::{HOT_KEY_MODIFIERS, MOD_CONTROL, RegisterHotKey, UnregisterHotKey};
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::w;

use super::bar::{self, Bar, Button};
use super::border;
use super::present::{ctrl_down, redraw, win_from};
use super::preview::Preview;
use super::session::Session;
use crate::overlay::geometry::SelRect;
use crate::overlay::ui::Ui;
use crate::theme::Palette;

/// 定时器 ID。
const TIMER_ID: usize = 1;

/// 全局热键 ID（执行窗口无焦点，键盘走全局热键；Enter/Esc/Ctrl+Z）。
const HK_ENTER: i32 = 301;
const HK_ESC: i32 = 302;
const HK_UNDO: i32 = 303;
/// 每拍间隔（毫秒）。
const TICK_MS: u32 = 90;

/// 注册/注销执行窗口的全局热键。
fn register_hotkeys(hwnd: HWND) {
    unsafe {
        let _ = RegisterHotKey(Some(hwnd), HK_ENTER, HOT_KEY_MODIFIERS(0), 0x0D);
        let _ = RegisterHotKey(Some(hwnd), HK_ESC, HOT_KEY_MODIFIERS(0), 0x1B);
        let _ = RegisterHotKey(Some(hwnd), HK_UNDO, MOD_CONTROL, 0x5A);
    }
}

fn unregister_hotkeys(hwnd: HWND) {
    unsafe {
        let _ = UnregisterHotKey(Some(hwnd), HK_ENTER);
        let _ = UnregisterHotKey(Some(hwnd), HK_ESC);
        let _ = UnregisterHotKey(Some(hwnd), HK_UNDO);
    }
}

/// 执行窗口运行时状态。
/// 字段为 `pub(super)`：绘制/上屏在 `present` 模块中实现（控制单文件规模）。
pub(super) struct Win {
    pub(super) session: Box<Session>,
    /// 视口边框窗口（M4，让用户看清截取范围）。
    pub(super) border_hwnd: Option<HWND>,
    /// 拼接结果预览窗（LNG-3，屏幕左右剩余空间放不下时为 None）。
    pub(super) preview: Option<Preview>,
    /// 状态条几何。
    pub(super) bar: Bar,
    /// 主题调色板（与工具栏一致）。
    pub(super) palette: &'static Palette,
    /// 上屏用内存 DC 与其 DIB。
    pub(super) mem_dc: HDC,
    pub(super) dib_bmp: HBITMAP,
    pub(super) dib_bits: *mut u8,
    /// 状态条像素尺寸。
    pub(super) w: i32,
    pub(super) h: i32,
    /// 是否有待上屏的变更。
    pub(super) dirty: bool,
}

impl Drop for Win {
    fn drop(&mut self) {
        unsafe {
            if !self.dib_bmp.0.is_null() {
                let _ = DeleteObject(self.dib_bmp.into());
            }
            if !self.mem_dc.0.is_null() {
                let _ = DeleteDC(self.mem_dc);
            }
        }
    }
}

/// 启动长截图执行并阻塞到完成/取消。完成后返回拼接结果；取消返回 None。
pub fn run(
    x: i32,
    y: i32,
    w: u32,
    h: u32,
    hinstance: windows::Win32::Foundation::HINSTANCE,
    max_height: u32,
    palette: &'static Palette,
    ui_scale: crate::settings::UiScale,
) -> Option<tiny_skia::Pixmap> {
    let session = match Session::new(x, y, w, h, max_height) {
        Ok(s) => s,
        Err(e) => {
            crate::app::message_box(None, &format!("长截图初始化失败：{e}"), MB_OK | MB_ICONERROR);
            return None;
        }
    };

    // 界面缩放：与截图覆盖层同一设置（手动倍率或按光标所在显示器自动，CAP-5）
    let ui = Ui::resolve(ui_scale);

    // 视口边框：提前创建让用户立刻看到截取范围。
    let border_hwnd = border::spawn(x, y, w, h, hinstance, ui);

    unsafe {
        let wc = WNDCLASSW {
            style: CS_HREDRAW | CS_VREDRAW,
            lpfnWndProc: Some(wndproc),
            hInstance: hinstance,
            hCursor: LoadCursorW(None, IDC_ARROW).unwrap_or_default(),
            lpszClassName: w!("airsnip.longshot"),
            ..Default::default()
        };
        let _ = RegisterClassW(&wc);

        let screen_h = GetSystemMetrics(SM_CYVIRTUALSCREEN);
        let bar = bar::layout(x, w, y, h, screen_h, ui);
        // 预览窗：在视口左右剩余空间显示拼接缩略图（放不下则不显示，长截图照常）。
        // 先于状态条创建，让状态条在 Z 序上位于预览之上。
        let preview = Preview::spawn(
            SelRect {
                x,
                y,
                w: w as i32,
                h: h as i32,
            },
            hinstance,
            palette,
            ui,
        );
        let (bw, bh) = (bar.rect.w, bar.rect.h);
        let (bx, by) = (bar.rect.x, bar.rect.y);

        let hwnd = CreateWindowExW(
            // WS_EX_LAYERED：UpdateLayeredWindow 提交带圆角透明的像素（与工具栏一致）；
            // WS_EX_NOACTIVATE：不抢焦点（手动模式下用户自行在目标页面滚动，状态条不抢焦点）。
            WS_EX_LAYERED | WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
            w!("airsnip.longshot"),
            w!("长截图"),
            WS_POPUP,
            bx,
            by,
            bw,
            bh,
            None,
            None,
            Some(hinstance),
            None,
        )
        .map_err(|e| {
            crate::app::message_box(None, &format!("创建长截图窗口失败：{e}"), MB_OK | MB_ICONERROR);
        })
        .ok()?;
        // 执行窗口禁输入法：中文 IME 会截走 Enter/Esc/Space 等按键。
        let _ = ImmAssociateContext(hwnd, HIMC(std::ptr::null_mut()));

        let (mem_dc, dib_bmp, dib_bits) = crate::overlay::surface::create_dib_surface(bw as u32, bh as u32).ok()?;
        let win = Box::new(Win {
            session: Box::new(session),
            border_hwnd,
            preview,
            bar,
            palette,
            mem_dc,
            dib_bmp,
            dib_bits,
            w: bw,
            h: bh,
            dirty: true,
        });
        let raw = Box::into_raw(win);
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, raw as isize);
        let _ = SetWindowPos(hwnd, Some(HWND_TOPMOST), bx, by, bw, bh, SWP_SHOWWINDOW);
        register_hotkeys(hwnd);
        let _ = SetTimer(Some(hwnd), TIMER_ID, TICK_MS, None);
        // 首帧立即上屏。
        redraw(hwnd);

        let mut msg = MSG::default();
        loop {
            let ret = GetMessageW(&mut msg, None, 0, 0);
            if ret.0 == 0 {
                break;
            }
            let _ = TranslateMessage(&msg);
            let _ = DispatchMessageW(&msg);
            if msg.hwnd == hwnd && msg.message == WM_DESTROY {
                break;
            }
        }

        // 局部持有 Box 指针，不重新查询窗口 userdata（销毁后不可靠，见 app 模块教训）。
        let result = if !(*raw).session.cancelled {
            (*raw).session.result().ok()
        } else {
            None
        };
        unregister_hotkeys(hwnd);
        if let Some(bh) = (*raw).border_hwnd {
            border::destroy(bh);
        }
        let _ = Box::from_raw(raw);
        result
    }
}

/// 完成：先补抓一帧（用户滚动后立刻按 Enter 时，最后一屏还没被定时拍捕获），再收尾。
fn finish(win: &mut Win) {
    win.session.tick();
    win.session.finished = true;
}

/// 绘制状态条并提交到分层窗口（实现见 `present::redraw`）。
unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_DESTROY => {
            unsafe {
                let _ = KillTimer(Some(hwnd), TIMER_ID);
                PostQuitMessage(0);
            }
            LRESULT(0)
        }
        WM_TIMER => {
            if let Some(win) = win_from(hwnd) {
                let changed = win.session.tick();
                if changed {
                    win.dirty = true;
                }
                if win.dirty {
                    redraw(hwnd);
                }
                if win.session.finished || win.session.cancelled {
                    unsafe {
                        let _ = DestroyWindow(hwnd);
                    }
                }
            }
            LRESULT(0)
        }
        WM_LBUTTONDOWN => {
            if let Some(win) = win_from(hwnd) {
                let x = (lparam.0 & 0xFFFF) as u16 as i16 as i32;
                let y = ((lparam.0 >> 16) & 0xFFFF) as u16 as i16 as i32;
                match win.bar.hit(x, y) {
                    Some(Button::Undo) => win.session.undo(),
                    Some(Button::Done) => finish(win),
                    Some(Button::Cancel) => win.session.cancelled = true,
                    None => {}
                }
                win.dirty = true;
                redraw(hwnd);
                if win.session.finished || win.session.cancelled {
                    unsafe {
                        let _ = DestroyWindow(hwnd);
                    }
                }
            }
            LRESULT(0)
        }
        WM_KEYDOWN => {
            if let Some(win) = win_from(hwnd) {
                match wparam.0 as u32 {
                    0x0D => finish(win),
                    0x1B => win.session.cancelled = true,
                    0x5A if ctrl_down() => win.session.undo(),
                    _ => {}
                }
                win.dirty = true;
                redraw(hwnd);
            }
            LRESULT(0)
        }
        WM_HOTKEY => {
            if let Some(win) = win_from(hwnd) {
                match wparam.0 as i32 {
                    HK_ENTER => finish(win),
                    HK_ESC => win.session.cancelled = true,
                    HK_UNDO => win.session.undo(),
                    _ => {}
                }
                win.dirty = true;
                redraw(hwnd);
            }
            LRESULT(0)
        }
        _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
    }
}
