// SPDX-License-Identifier: GPL-3.0-only
//! 贴图落点计算（PIN-1）：原位 / 居中两种策略。
//!
//! **居中必须按「一块显示器」算，不能按虚拟屏**：`SM_CXVIRTUALSCREEN` 是所有显示器的
//! 并集，副屏挂在左侧或右侧时它的中心落在两块屏的接缝上，贴图就会一半在左屏一半在右屏
//! （issue #152 追问：主屏在右时 F3 贴图出现在两个屏幕中间）。原先只有单屏 / 主屏在左上
//! 时二者才等价，所以一直没暴露。原位贴图仍按虚拟屏限幅 —— 选区本来就可能跨屏。

use windows::Win32::Foundation::POINT;
use windows::Win32::Graphics::Gdi::{GetMonitorInfoW, MONITOR_DEFAULTTONEAREST, MONITORINFO, MonitorFromPoint};
use windows::Win32::UI::WindowsAndMessaging::{
    GetCursorPos, GetSystemMetrics, SM_CXVIRTUALSCREEN, SM_CYVIRTUALSCREEN, SM_XVIRTUALSCREEN, SM_YVIRTUALSCREEN,
};

use crate::settings::PinPosition;

/// 贴图窗口左上角（屏幕坐标）。
/// `at` 为选区屏幕位置（Original 用）；`size` 为像素图尺寸；`n` 是同屏已有贴图数量（级联偏移）。
pub(super) fn resolve(pin_place: PinPosition, at: Option<(i32, i32)>, size: (i32, i32), n: i32) -> (i32, i32) {
    match (pin_place, at) {
        // 原位：坐标已含虚拟屏偏移（覆盖层画布 → 屏幕，见 overlay/annotate/output.rs 的
        // to_screen），只限幅回屏幕上、不再做任何居中
        (PinPosition::Original, Some((sx, sy))) => clamp_in(virtual_screen(), (sx, sy), size),
        // 居中（剪贴板贴图 / 设置了居中）：按光标所在显示器，跨屏接缝处不算数
        _ => centered_in(cursor_screen().unwrap_or_else(virtual_screen), size, n),
    }
}

/// 在 `screen` 内居中的左上角，第 `n` 张向右下错开 20px 避免完全重叠，结果钳回屏内。
fn centered_in(screen: (i32, i32, i32, i32), size: (i32, i32), n: i32) -> (i32, i32) {
    let (vx, vy, vw, vh) = screen;
    let (w, h) = size;
    clamp_in(
        screen,
        (vx + vw / 2 - w / 2 + n * 20, vy + vh / 2 - h / 2 + n * 20),
        size,
    )
}

/// 把窗口左上角钳进 `screen`（留 4px 边），贴图比屏幕还大时贴左上角。
fn clamp_in(screen: (i32, i32, i32, i32), (px, py): (i32, i32), (w, h): (i32, i32)) -> (i32, i32) {
    let (vx, vy, vw, vh) = screen;
    (
        px.clamp(vx + 4, (vx + vw - w - 4).max(vx + 4)),
        py.clamp(vy + 4, (vy + vh - h - 4).max(vy + 4)),
    )
}

/// 光标所在显示器的矩形（物理像素）；取不到光标或显示器信息时返回 None。
fn cursor_screen() -> Option<(i32, i32, i32, i32)> {
    unsafe {
        let mut pt = POINT::default();
        GetCursorPos(&mut pt).ok()?;
        let monitor = MonitorFromPoint(pt, MONITOR_DEFAULTTONEAREST);
        if monitor.0.is_null() {
            return None;
        }
        let mut info = MONITORINFO {
            cbSize: std::mem::size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        if !GetMonitorInfoW(monitor, &mut info).as_bool() {
            return None;
        }
        let r = info.rcMonitor;
        Some((r.left, r.top, (r.right - r.left).max(1), (r.bottom - r.top).max(1)))
    }
}

/// 虚拟屏幕矩形（多显示器统一坐标）。
fn virtual_screen() -> (i32, i32, i32, i32) {
    unsafe {
        (
            GetSystemMetrics(SM_XVIRTUALSCREEN),
            GetSystemMetrics(SM_YVIRTUALSCREEN),
            GetSystemMetrics(SM_CXVIRTUALSCREEN),
            GetSystemMetrics(SM_CYVIRTUALSCREEN),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SINGLE: (i32, i32, i32, i32) = (0, 0, 1920, 1080);
    /// 副屏挂在左侧：虚拟屏原点为负，接缝在 x = 0。
    const SECOND_LEFT: (i32, i32, i32, i32) = (-1920, 0, 1920, 1080);
    /// 主屏在右的布局（issue #152 提问者）：虚拟屏是 (-1920, 0, 3840, 1080)。
    const VIRTUAL_WIDE: (i32, i32, i32, i32) = (-1920, 0, 3840, 1080);

    #[test]
    fn centered_in_single_screen_is_screen_center() {
        assert_eq!(centered_in(SINGLE, (400, 300), 0), (760, 390));
    }

    /// 主屏在右、副屏在左：以光标屏居中时贴图整块落在该屏内，不会踩到接缝。
    #[test]
    fn centered_in_negative_origin_screen_stays_inside() {
        let (x, y) = centered_in(SECOND_LEFT, (400, 300), 0);
        assert_eq!((x, y), (-1160, 390), "副屏居中");
        assert!(x >= SECOND_LEFT.0 && x + 400 <= SECOND_LEFT.0 + SECOND_LEFT.2);
    }

    /// 虚拟屏居中会把贴图横跨接缝（x 落在两屏之间）——这正是 issue #152 追问的现象，
    /// 与「按光标屏居中」对照，守住两条策略的区别。
    #[test]
    fn virtual_screen_center_straddles_the_seam() {
        let (x, _) = centered_in(VIRTUAL_WIDE, (400, 300), 0);
        assert_eq!(x, -200, "虚拟屏中心跨接缝");
        let (x, _) = centered_in(SECOND_LEFT, (400, 300), 0);
        assert_eq!(x, -1160, "光标屏中心不跨");
    }

    #[test]
    fn centered_in_cascades_and_clamps() {
        // 第 3 张整体右下移 60px
        assert_eq!(centered_in(SINGLE, (400, 300), 3), (820, 450));
        // 偏移到越界时钳回屏内右 / 下边
        assert_eq!(centered_in(SINGLE, (400, 300), 1000), (1516, 776));
        // 贴图比屏幕还大：贴左上角
        assert_eq!(centered_in(SINGLE, (4000, 3000), 5), (4, 4));
    }

    #[test]
    fn clamp_in_keeps_original_selection_inside() {
        // 原位贴图：合法坐标原样返回
        assert_eq!(clamp_in(VIRTUAL_WIDE, (-1000, 200), (400, 300)), (-1000, 200));
        // 越出虚拟屏左边界时拉回 4px
        assert_eq!(clamp_in(VIRTUAL_WIDE, (-2000, 200), (400, 300)), (-1916, 200));
    }
}
