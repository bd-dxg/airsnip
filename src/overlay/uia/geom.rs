// SPDX-License-Identifier: GPL-3.0-only
//! UIA 的矩形工具（CAP-6 二级 / ADR 0039）：全部是屏幕物理像素 `(x, y, w, h)` 的纯计算，
//! 不碰 COM，便于单测。含「自绘程序兜底用的客户区」（UIA 一点子元素都没给时的那一档）。

use windows::Win32::Foundation::{HWND, POINT, RECT};

/// 屏幕物理像素矩形：(x, y, w, h)。
pub(super) type Rect4 = (i32, i32, i32, i32);

/// 点是否落在矩形内（半开区间）。
pub(super) fn contains((x, y, w, h): Rect4, (px, py): (i32, i32)) -> bool {
    px >= x && px < x + w && py >= y && py < y + h
}

/// 矩形裁剪（半开区间），不相交返回 `None`。
pub(super) fn clip(r: Rect4, frame: Rect4) -> Option<Rect4> {
    let x0 = r.0.max(frame.0);
    let y0 = r.1.max(frame.1);
    let x1 = (r.0 + r.2).min(frame.0 + frame.2);
    let y1 = (r.1 + r.3).min(frame.1 + frame.3);
    (x1 > x0 && y1 > y0).then_some((x0, y0, x1 - x0, y1 - y0))
}

/// 客户区作为兜底层级（ADR 0039 §13）：UIA 什么都没给时用。
/// 与整窗几乎相同（无边框窗口）就返回 `None`——这一档没有信息量。
pub(super) fn useful_client_level(client: Rect4, frame: Rect4) -> Option<Rect4> {
    let c = clip(client, frame)?;
    ((c.2 - frame.2).abs() > 4 || (c.3 - frame.3).abs() > 4).then_some(c)
}

/// 取窗口客户区（屏幕物理像素）。
pub(super) fn client_area(hwnd: isize, frame: Rect4) -> Option<Rect4> {
    use windows::Win32::Graphics::Gdi::ClientToScreen;
    use windows::Win32::UI::WindowsAndMessaging::GetClientRect;
    let h = HWND(hwnd as *mut core::ffi::c_void);
    let mut rc = RECT::default();
    unsafe { GetClientRect(h, &mut rc) }.ok()?;
    let (w, height) = (rc.right - rc.left, rc.bottom - rc.top);
    if w <= 0 || height <= 0 {
        return None;
    }
    let mut origin = POINT { x: 0, y: 0 };
    if !unsafe { ClientToScreen(h, &mut origin) }.as_bool() {
        return None;
    }
    useful_client_level((origin.x, origin.y, w, height), frame)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 裁剪：与窗口求交，完全不相交返回 None。
    #[test]
    fn clip_intersects_frame() {
        let frame = (100, 100, 800, 600);
        assert_eq!(clip((150, 200, 100, 50), frame), Some((150, 200, 100, 50)));
        assert_eq!(clip((50, 50, 200, 200), frame), Some((100, 100, 150, 150)));
        assert_eq!(clip((2000, 2000, 10, 10), frame), None);
    }

    /// 客户区兜底：比整窗小就用，与整窗几乎相同就丢掉（无边框窗口）。
    #[test]
    fn client_level_only_when_smaller() {
        let frame = (100, 100, 800, 600);
        // 正常有标题栏的窗口：客户区比整窗小
        assert_eq!(
            useful_client_level((108, 131, 784, 561), frame),
            Some((108, 131, 784, 561))
        );
        // 无边框窗口：客户区 = 整窗，不做这一档
        assert_eq!(useful_client_level((100, 100, 800, 600), frame), None);
        // 差 2px 也算重复
        assert_eq!(useful_client_level((100, 100, 798, 598), frame), None);
    }

    /// 点包含判定：半开区间，与屏幕像素的习惯一致（右下边界不属于该元素）。
    /// 向下钻靠它选「包含光标的子元素」，写反会一直钻到叶子或一层也进不去。
    #[test]
    fn contains_is_half_open() {
        let r = (10, 10, 20, 20);
        assert!(contains(r, (10, 10)));
        assert!(contains(r, (29, 29)));
        assert!(!contains(r, (30, 20)));
        assert!(!contains(r, (20, 30)));
        assert!(!contains(r, (9, 10)));
    }
}
