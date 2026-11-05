// SPDX-License-Identifier: GPL-3.0-only
//! 气泡窗口消息分发（OCR-7～OCR-10）。
//!
//! 窗口是 `WS_POPUP | WS_CLIPCHILDREN` + `WS_EX_TOOLWINDOW`（不进任务栏、无系统标题栏），
//! 圆角靠 `SetWindowRgn`（见 `view::apply_size`）。文本区是原生 `EDIT` 子控件，
//! 键盘输入、输入法、选中、右键菜单全部由它自己处理，父窗口只管外壳。
//! 创建与定位见 `create`。

use windows::Win32::Foundation::{HGLOBAL, HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::Graphics::Gdi::{BeginPaint, EndPaint, HDC, PAINTSTRUCT};
use windows::Win32::UI::Input::KeyboardAndMouse::{ReleaseCapture, TME_LEAVE, TRACKMOUSEEVENT, TrackMouseEvent};
use windows::Win32::UI::WindowsAndMessaging::*;

use super::create::{focus, state_of};
use super::{Bubble, WM_OCR_DONE, clear_current, take_text};

/// 鼠标离开客户区（`WM_MOUSELEAVE` = 0x02A3）。
/// windows-rs 0.62 没有导出这个常量，只能自己定义。
const WM_MOUSELEAVE: u32 = 0x02A3;

pub(super) unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    // 销毁时释放 Box：窗口关掉后气泡状态不再需要
    if msg == WM_NCDESTROY {
        unsafe {
            let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA);
            if ptr != 0 {
                drop(Box::from_raw(ptr as *mut Bubble));
                SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
            }
        }
        clear_current();
        return unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) };
    }
    // 关闭先于取状态：`DestroyWindow` 会同步走 WM_NCDESTROY 释放 Box，
    // 若此时还持着 `&mut Bubble` 就成了悬空引用。
    if msg == WM_CLOSE {
        unsafe {
            let _ = DestroyWindow(hwnd);
        }
        return LRESULT(0);
    }
    let Some(bubble) = (unsafe { state_of(hwnd) }) else {
        return unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) };
    };
    match msg {
        WM_PAINT => {
            let mut ps = PAINTSTRUCT::default();
            unsafe {
                let _ = BeginPaint(hwnd, &mut ps);
            }
            bubble.paint();
            unsafe {
                let _ = EndPaint(hwnd, &ps);
            }
            LRESULT(0)
        }
        WM_ERASEBKGND => LRESULT(1),
        // 文本区配色：可编辑走 CTLCOLOREDIT，只读走 CTLCOLORSTATIC（系统按只读状态选消息）
        WM_CTLCOLOREDIT | WM_CTLCOLORSTATIC => {
            let hdc = HDC(wparam.0 as *mut _);
            bubble.ctl.paint_colors(hdc, bubble.palette);
            LRESULT(bubble.ctl.brush().0 as isize)
        }
        WM_COMMAND => {
            let code = ((wparam.0 >> 16) & 0xFFFF) as u32;
            // 程序自己 SetWindowTextW 也会触发 EN_CHANGE，重排由 `view` 那边做，
            // 这里只响应「用户真的改了字」。
            if code == EN_CHANGE && !bubble.syncing {
                bubble.relayout();
                bubble.paint();
            } else if code == EN_SETFOCUS || code == EN_KILLFOCUS {
                // 边框：有焦点时用强调色
                bubble.paint();
            }
            LRESULT(0)
        }
        // 拖动：卡片没有标题栏，文本区以外的空白处按下即拖窗口；按钮区交给命中处理
        WM_LBUTTONDOWN => {
            let (x, y) = point(lparam);
            if bubble.hit(x, y) != super::interact::Hit::Other {
                if bubble.on_mouse_down(x, y) {
                    bubble.paint();
                }
                return LRESULT(0);
            }
            unsafe {
                let _ = ReleaseCapture();
                let _ = SendMessageW(
                    hwnd,
                    WM_NCLBUTTONDOWN,
                    Some(WPARAM(HTCAPTION as usize)),
                    Some(LPARAM(0)),
                );
            }
            LRESULT(0)
        }
        WM_MOUSEMOVE => {
            let (x, y) = point(lparam);
            if bubble.update_hover(x, y) {
                bubble.paint();
            }
            // 请求下次离开时收到 WM_MOUSELEAVE（一次性注册）
            unsafe {
                let mut tme = TRACKMOUSEEVENT {
                    cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32,
                    dwFlags: TME_LEAVE,
                    hwndTrack: hwnd,
                    dwHoverTime: 0,
                };
                let _ = TrackMouseEvent(&mut tme);
            }
            LRESULT(0)
        }
        WM_MOUSELEAVE => {
            if bubble.clear_hover() {
                bubble.paint();
            }
            LRESULT(0)
        }
        // 「已复制」反馈到时：立刻停掉定时器，恢复按钮文案
        WM_TIMER if wparam.0 == super::TIMER_COPIED => {
            unsafe {
                let _ = KillTimer(Some(hwnd), super::TIMER_COPIED);
            }
            if bubble.copied {
                bubble.copied = false;
                bubble.paint();
            }
            LRESULT(0)
        }
        // 识别结果回到主线程（OCR-8）：就地替换「识别中…」
        WM_OCR_DONE => {
            if lparam.0 == 2 {
                // 载荷分配失败（内存不足）：不读 hg
                bubble.set_error("取字结果无法显示（内存不足）。");
            } else {
                let (text, lang) = take_text(HGLOBAL(wparam.0 as *mut _));
                if lparam.0 == 0 {
                    bubble.set_result(text, lang);
                } else {
                    bubble.set_error(&text);
                }
            }
            bubble.paint();
            // 只在卡片仍在前台时把焦点还给文本区：识别很快，但用户可能已经切走，
            // 无条件 `SetForegroundWindow` 会把人家刚切的窗口抢回来
            if unsafe { GetForegroundWindow() } == hwnd {
                focus(bubble);
            }
            LRESULT(0)
        }
        _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
    }
}

/// 客户区坐标（lparam 拆包）。
fn point(lparam: LPARAM) -> (i32, i32) {
    (
        (lparam.0 & 0xFFFF) as u16 as i16 as i32,
        ((lparam.0 >> 16) & 0xFFFF) as u16 as i16 as i32,
    )
}
