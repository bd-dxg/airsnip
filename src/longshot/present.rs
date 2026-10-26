// SPDX-License-Identifier: GPL-3.0-only
//! 长截图状态条上屏（LNG-1）：分层窗口像素提交（`UpdateLayeredWindow`）与
//! 窗口状态取回。绘制见 `bar`，执行流程见 `window`。

use windows::Win32::Foundation::{COLORREF, HWND, POINT, SIZE};
use windows::Win32::Graphics::Gdi::{AC_SRC_ALPHA, AC_SRC_OVER, BLENDFUNCTION, HDC};
use windows::Win32::UI::Input::KeyboardAndMouse::GetKeyState;
use windows::Win32::UI::WindowsAndMessaging::{
    GWLP_USERDATA, GetWindowLongPtrW, GetWindowRect, ULW_ALPHA, UpdateLayeredWindow,
};

use super::window::Win;

/// UpdateLayeredWindow 提交（窗口位置不变，只更新像素）。
pub(super) unsafe fn present(hwnd: HWND, mem_dc: HDC) {
    unsafe {
        let mut rect = windows::Win32::Foundation::RECT::default();
        let _ = GetWindowRect(hwnd, &mut rect);
        let ppt_dst = POINT {
            x: rect.left,
            y: rect.top,
        };
        let psize = SIZE {
            cx: rect.right - rect.left,
            cy: rect.bottom - rect.top,
        };
        let ppt_src = POINT { x: 0, y: 0 };
        let blend = BLENDFUNCTION {
            BlendOp: AC_SRC_OVER as u8,
            BlendFlags: 0,
            SourceConstantAlpha: 255,
            AlphaFormat: AC_SRC_ALPHA as u8,
        };
        let _ = UpdateLayeredWindow(
            hwnd,
            None,
            Some(&ppt_dst),
            Some(&psize),
            Some(mem_dc),
            Some(&ppt_src),
            COLORREF(0),
            Some(&blend),
            ULW_ALPHA,
        );
    }
}

/// RGBA（premultiplied）→ BGRA。
pub(super) fn premul_bgra(rgba: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(rgba.len());
    for px in rgba.chunks_exact(4) {
        out.extend_from_slice(&[px[2], px[1], px[0], px[3]]);
    }
    out
}

/// 从窗口用户数据取回运行时状态。
pub(super) fn win_from(hwnd: HWND) -> Option<&'static mut Win> {
    unsafe {
        let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA);
        if ptr == 0 { None } else { Some(&mut *(ptr as *mut Win)) }
    }
}

/// Ctrl 是否按下（Ctrl+Z 撤销）。
pub(super) fn ctrl_down() -> bool {
    unsafe { GetKeyState(0x11) as u16 & 0x8000 != 0 }
}
