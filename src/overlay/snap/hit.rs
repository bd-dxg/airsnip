// SPDX-License-Identifier: GPL-3.0-only
//! 窗口命中查询（CAP-6 / ADR 0037，issue #62）：Z 序枚举顶层窗口，第一个「可见、含光标、不是本覆盖层」
//! 的窗口即目标。边界取 DWM 扩展边框——`GetWindowRect` 含 Win10+ 不可见的缩放边框与阴影，
//! 吸出来的矩形会比视觉窗口大若干像素。
//!
//! 不用 `WindowFromPoint`：覆盖层铺满虚拟屏且置顶，命中的永远是它自己；issue 建议的
//! 「临时加 `WS_EX_TRANSPARENT` 再查」存在样式回滚窗口，期间覆盖层对外部鼠标「不存在」。

use windows::Win32::Foundation::{HWND, RECT};
use windows::Win32::Graphics::Dwm::{DWMWA_CLOAKED, DWMWA_EXTENDED_FRAME_BOUNDS, DwmGetWindowAttribute};
use windows::Win32::UI::WindowsAndMessaging::{
    GW_HWNDNEXT, GetClassNameW, GetTopWindow, GetWindow, GetWindowRect, IsIconic, IsWindowVisible,
};

use super::super::geometry::SelRect;

/// 桌面外壳窗口类名：铺满整屏，吸上去等于「整屏候选」，还会抢掉「按在空白处重新框选」。
const DESKTOP_CLASSES: [&str; 2] = ["Progman", "WorkerW"];

/// 光标（屏幕物理像素）落在哪个顶层窗口上：返回 `(窗口句柄, 覆盖层坐标矩形)`。
/// `overlay` 为覆盖层自身句柄（跳过）；`origin` 是抓屏原点，`size` 是虚拟屏尺寸（用于裁剪）。
pub(super) fn window_at(
    overlay: HWND,
    sx: i32,
    sy: i32,
    origin: (i32, i32),
    size: (i32, i32),
) -> Option<(isize, SelRect)> {
    let mut hwnd = unsafe { GetTopWindow(None) }.ok()?;
    while !hwnd.is_invalid() {
        // 先做最便宜的两步（拿矩形 + 判含光标），绝大多数窗口在这一步就被排除，
        // 类名 / cloak / DWM 边界这些贵的查询只会落在真正压住光标的那个窗口上
        if hwnd != overlay
            && let Some(hit) = raw_window_rect(hwnd)
            && contains(hit, sx, sy)
            && is_candidate(hwnd)
            // 命中判定用 `GetWindowRect`（便宜），最终矩形才问 DWM（贵）；
            // DWM 拿不到有意义的矩形（DirectX 独占全屏等）就整个跳过，不回退 `GetWindowRect`
            && let Some(frame) = dwm_frame_rect(hwnd)
            && let Some(rect) = clip(frame, origin, size)
        {
            return Some((hwnd.0 as isize, rect));
        }
        hwnd = unsafe { GetWindow(hwnd, GW_HWNDNEXT) }.ok()?;
    }
    None
}

/// 候选窗口筛选：可见、未最小化、未被 DWM 隐身、不是桌面壳。
fn is_candidate(hwnd: HWND) -> bool {
    unsafe {
        if !IsWindowVisible(hwnd).as_bool() || IsIconic(hwnd).as_bool() {
            return false;
        }
    }
    !is_cloaked(hwnd) && !is_desktop(hwnd)
}

/// DWM 隐身的窗口（`DWMWA_CLOAKED`）：UWP 挂起、虚拟桌面切换后残留的幽灵窗口，
/// 视觉上不存在但仍在 Z 序里，不跳过就会吸到看不见的矩形。
fn is_cloaked(hwnd: HWND) -> bool {
    let mut cloaked = 0u32;
    let ok = unsafe { DwmGetWindowAttribute(hwnd, DWMWA_CLOAKED, (&raw mut cloaked).cast(), size_of::<u32>() as u32) }
        .is_ok();
    ok && cloaked != 0
}

fn is_desktop(hwnd: HWND) -> bool {
    let mut buf = [0u16; 64];
    let n = unsafe { GetClassNameW(hwnd, &mut buf) };
    if n <= 0 {
        return false;
    }
    let name = String::from_utf16_lossy(&buf[..n as usize]);
    DESKTOP_CLASSES.iter().any(|c| c.eq_ignore_ascii_case(&name))
}

/// 窗口矩形（屏幕物理像素），仅用于命中判定。
fn raw_window_rect(hwnd: HWND) -> Option<(i32, i32, i32, i32)> {
    let mut rc = RECT::default();
    unsafe { GetWindowRect(hwnd, &mut rc) }.ok()?;
    nonempty(rc)
}

/// DWM 扩展边框（屏幕物理像素）：去掉不可见的缩放边框与阴影，与视觉边界一致。
fn dwm_frame_rect(hwnd: HWND) -> Option<(i32, i32, i32, i32)> {
    let mut rc = RECT::default();
    unsafe {
        DwmGetWindowAttribute(
            hwnd,
            DWMWA_EXTENDED_FRAME_BOUNDS,
            (&raw mut rc).cast(),
            size_of::<RECT>() as u32,
        )
    }
    .ok()?;
    nonempty(rc)
}

fn nonempty(rc: RECT) -> Option<(i32, i32, i32, i32)> {
    let (w, h) = (rc.right - rc.left, rc.bottom - rc.top);
    (w > 0 && h > 0).then_some((rc.left, rc.top, w, h))
}

/// 点是否在矩形内（半开区间）。
fn contains((x, y, w, h): (i32, i32, i32, i32), px: i32, py: i32) -> bool {
    px >= x && px < x + w && py >= y && py < y + h
}

/// 屏幕矩形 → 覆盖层坐标，并裁掉越出虚拟屏的部分（跨屏窗口、负坐标副屏）。
fn clip(rect: (i32, i32, i32, i32), origin: (i32, i32), size: (i32, i32)) -> Option<SelRect> {
    let (x, y, w, h) = rect;
    let x0 = (x - origin.0).max(0);
    let y0 = (y - origin.1).max(0);
    let x1 = (x - origin.0 + w).min(size.0);
    let y1 = (y - origin.1 + h).min(size.1);
    (x1 > x0 && y1 > y0).then_some(SelRect {
        x: x0,
        y: y0,
        w: x1 - x0,
        h: y1 - y0,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 主屏原点 (0,0)：矩形原样映射。
    #[test]
    fn clip_on_primary_origin() {
        let r = clip((100, 200, 300, 400), (0, 0), (1920, 1080)).expect("应命中");
        assert_eq!((r.x, r.y, r.w, r.h), (100, 200, 300, 400));
    }

    /// 左侧副屏（负坐标）：抓屏原点是 (-1920, 0)，窗口仍在正确位置。
    #[test]
    fn clip_with_negative_origin() {
        let r = clip((-1800, 100, 400, 300), (-1920, 0), (3840, 1080)).expect("应命中");
        assert_eq!((r.x, r.y, r.w, r.h), (120, 100, 400, 300));
    }

    /// 跨屏窗口超出虚拟屏的部分被裁掉，不能出现负宽高。
    #[test]
    fn clip_trims_oversize_window() {
        let r = clip((-100, -50, 400, 300), (0, 0), (1920, 1080)).expect("应命中");
        assert_eq!((r.x, r.y, r.w, r.h), (0, 0, 300, 250));
        let r = clip((1800, 900, 500, 500), (0, 0), (1920, 1080)).expect("应命中");
        assert_eq!((r.x, r.y, r.w, r.h), (1800, 900, 120, 180));
    }

    /// 完全在虚拟屏之外 / 零尺寸：不产生候选。
    #[test]
    fn clip_drops_empty() {
        assert!(clip((3000, 100, 400, 300), (0, 0), (1920, 1080)).is_none());
        assert!(clip((0, 0, 0, 100), (0, 0), (1920, 1080)).is_none());
        assert!(clip((100, 100, 0, 0), (0, 0), (1920, 1080)).is_none());
    }

    /// 命中判定是半开区间：右/下边界上的点不属于该窗口（与 `SelRect::contains` 一致）。
    #[test]
    fn contains_is_half_open() {
        let r = (10, 10, 20, 20);
        assert!(contains(r, 10, 10));
        assert!(contains(r, 29, 29));
        assert!(!contains(r, 30, 20));
        assert!(!contains(r, 20, 30));
        assert!(!contains(r, 9, 10));
    }
}
