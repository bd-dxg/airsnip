// SPDX-License-Identifier: GPL-3.0-only
//! 气泡窗口创建与定位（OCR-7）：注册窗口类、创建/复用窗口、放到选区附近、抢焦点。
//!
//! 从 `wnd` 拆出，保证单文件不超 300 行；消息分发仍在 `wnd`。

use std::sync::atomic::{AtomicBool, Ordering};

use windows::Win32::Foundation::{HWND, POINT, RECT};
use windows::Win32::Graphics::Gdi::{GetMonitorInfoW, MONITOR_DEFAULTTONEAREST, MONITORINFO, MonitorFromPoint};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Input::KeyboardAndMouse::SetFocus;
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::{PCWSTR, w};

use super::ctl::Ctl;
use super::layout::layout;
use super::wnd::wndproc;
use super::{Bubble, State, body_size, current, set_current};
use crate::overlay::ui::Ui;
use crate::theme::Palette;

/// 窗口类名。
const CLASS: PCWSTR = w!("airsnip.bubble");
/// 类是否已注册（进程内只注册一次）。
static REGISTERED: AtomicBool = AtomicBool::new(false);

/// 打开（或复用）气泡窗口。
pub(super) fn ensure_window(palette: &'static Palette, ui: Ui) -> Option<HWND> {
    if let Some(hwnd) = current() {
        return Some(hwnd);
    }
    register_class()?;
    let hinstance = unsafe { GetModuleHandleW(None) }.ok()?.into();
    let hwnd = unsafe {
        CreateWindowExW(
            // 不再用 WS_EX_LAYERED：分层窗口不渲染子控件（文本区是原生 EDIT），
            // 圆角改 SetWindowRgn，见 `view::apply_size`
            WS_EX_TOOLWINDOW,
            CLASS,
            w!("AirSnip 取字"),
            // WS_CLIPCHILDREN：上屏是整幅 BitBlt，不能让父窗口盖到 EDIT 上
            WS_POPUP | WS_CLIPCHILDREN,
            0,
            0,
            10,
            10,
            None,
            None,
            Some(hinstance),
            None,
        )
    }
    .ok()?;

    let ctl = match Ctl::create(hwnd, body_size(ui), palette) {
        Some(c) => c,
        None => {
            unsafe {
                let _ = DestroyWindow(hwnd);
            }
            return None;
        }
    };
    let bubble = Bubble {
        hwnd,
        ui,
        palette,
        state: State::Loading,
        ctl,
        layout: layout(ui, 0),
        copied: false,
        pinned: false,
        auto_copy: false,
        main_hwnd: HWND::default(),
        hover: None,
        max_body: layout(ui, 0).body.h,
        syncing: false,
        dib_bmp: Default::default(),
        mem_dc: Default::default(),
        dib_bits: std::ptr::null_mut(),
    };
    let raw = Box::into_raw(Box::new(bubble));
    unsafe {
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, raw as isize);
    }
    set_current(hwnd);
    Some(hwnd)
}

fn register_class() -> Option<()> {
    if REGISTERED.load(Ordering::Acquire) {
        return Some(());
    }
    let hinstance = unsafe { GetModuleHandleW(None) }.ok()?.into();
    let wc = WNDCLASSW {
        lpfnWndProc: Some(wndproc),
        hInstance: hinstance,
        // 类光标必须给：不设时 DefWindowProc 不改光标，会沿用上一个（启动残留的忙碌光标），
        // 表现为鼠标移入卡片一直转圈。文本区内由 EDIT 自己换成 I 形。
        hCursor: unsafe { LoadCursorW(None, IDC_ARROW) }.unwrap_or_default(),
        lpszClassName: CLASS,
        ..Default::default()
    };
    // 注册成功后才置位：失败（极罕见）时下次取字还能重试
    if unsafe { RegisterClassW(&wc) } == 0 {
        return None;
    }
    REGISTERED.store(true, Ordering::Release);
    Some(())
}

/// 把气泡放到选区附近：优先下方，塞不下翻到上方，最后夹进显示器工作区。
pub(super) fn place_at(hwnd: HWND, bubble: &Bubble, sel: (i32, i32, i32, i32)) {
    let (bw, bh) = (bubble.layout.w, bubble.layout.h);
    let work = work_area(sel.0 + sel.2 / 2, sel.1 + sel.3 / 2);
    let (x, y) = place_xy(sel, (bw, bh), bubble.ui.px(8.0), work);
    unsafe {
        // 不用 TOPMOST：气泡是普通窗口，用户切到别的窗口工作时它应正常退到后面
        let _ = SetWindowPos(hwnd, Some(HWND_TOP), x, y, bw, bh, SWP_SHOWWINDOW | SWP_NOACTIVATE);
    }
}

/// 卡片左上角：选区下方优先，塞不下翻到上方，最后夹进工作区。
///
/// 纯函数方便单测：之前只在「上方翻位」时夹了 `work.top`，没夹 `work.bottom`，
/// 卡片比屏幕高时（选区靠下、内容很多）会溢出屏幕。
fn place_xy(sel: (i32, i32, i32, i32), card: (i32, i32), gap: i32, work: RECT) -> (i32, i32) {
    let (sx, sy, _, sh) = sel;
    let (bw, bh) = card;
    let x = sx.clamp(work.left, (work.right - bw).max(work.left));
    let below = sy + sh + gap;
    let above = sy - gap - bh;
    let y = if below + bh <= work.bottom { below } else { above };
    let y = y.clamp(work.top, (work.bottom - bh).max(work.top));
    (x, y)
}

/// 取点所在显示器的工作区（排除任务栏）。
pub(super) fn work_area(x: i32, y: i32) -> RECT {
    let mut info = MONITORINFO {
        cbSize: std::mem::size_of::<MONITORINFO>() as u32,
        ..Default::default()
    };
    unsafe {
        let m = MonitorFromPoint(POINT { x, y }, MONITOR_DEFAULTTONEAREST);
        if GetMonitorInfoW(m, &mut info).as_bool() {
            return info.rcWork;
        }
    }
    RECT {
        left: 0,
        top: 0,
        right: 1920,
        bottom: 1080,
    }
}

/// 抢焦点（OCR-7：气泡出现即可直接 Ctrl+C / 输入）。
///
/// 焦点直接给文本区：输入法候选框、光标、`Ctrl+V` 都挂在这个控件上。
pub(super) fn focus(bubble: &Bubble) {
    unsafe {
        let _ = SetForegroundWindow(bubble.hwnd);
        let _ = SetFocus(Some(bubble.ctl.hwnd));
    }
}

/// 取窗口状态指针。
pub(super) unsafe fn state_of(hwnd: HWND) -> Option<&'static mut Bubble> {
    let ptr = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) };
    (ptr != 0).then(|| unsafe { &mut *(ptr as *mut Bubble) })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn work() -> RECT {
        RECT {
            left: 0,
            top: 0,
            right: 1920,
            bottom: 1080,
        }
    }

    /// 选区靠屏幕下方、下方塞不下卡片时，翻到上方且不溢出。
    #[test]
    fn card_above_selection_when_no_room_below() {
        let (x, y) = place_xy((100, 900, 300, 120), (420, 300), 8, work());
        assert_eq!(x, 100);
        assert_eq!(y, 900 - 8 - 300);
        assert!(y + 300 <= work().bottom, "卡片溢出屏幕底部：{y}");
    }

    /// 卡片比屏幕还高时（理论上被 `max_body` 拦住）也不能溢出：夹到顶部。
    #[test]
    fn card_taller_than_work_area_clamps_to_top() {
        let (_, y) = place_xy((0, 500, 100, 100), (420, 2000), 8, work());
        assert_eq!(y, 0);
    }

    /// 下方装得下就放下方（默认行为不能变）。
    #[test]
    fn card_below_selection_when_room() {
        let (_, y) = place_xy((100, 100, 300, 100), (420, 300), 8, work());
        assert_eq!(y, 100 + 100 + 8);
    }

    /// 选区贴近右边时卡片向左夹，不溢出。
    #[test]
    fn card_clamped_horizontally() {
        let (x, _) = place_xy((1800, 100, 120, 100), (420, 300), 8, work());
        assert_eq!(x, 1920 - 420);
    }
}
