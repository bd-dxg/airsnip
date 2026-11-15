// SPDX-License-Identifier: GPL-3.0-only
//! 快速选区键（ADR 0040）：**全屏**（默认 F）与**上次区域**（默认 R）。
//!
//! 两者都只在「当前还没有选区」时生效 —— 已经有选区（尤其已经画了标注）时按下去什么
//! 也不做，避免误按把正在编辑的内容换掉。贴图编辑模式（ADR 0028，选区锁定）同理不生效。
//!
//! 全屏取**光标所在显示器**的整屏矩形（`rcMonitor`，含任务栏）：覆盖层铺满整个虚拟屏，
//! 多显示器下要的是「鼠标在哪块屏就框哪块屏」。
//!
//! 上次区域记在进程内（`LAST_AREA`），只记用户自己确定的选区（手框松手、窗口吸附单击），
//! **不记** F / R 自己产生的选区 —— 否则按一次全屏就把上次手框的位置盖掉，R 再也回不去。
//! 恢复时与当前虚拟屏求交：显示器被拔掉 / 分辨率改了自动裁小，交集为空则静默不响应。

use std::sync::Mutex;
use windows::Win32::Foundation::POINT;
use windows::Win32::Graphics::Gdi::{GetMonitorInfoW, MONITOR_DEFAULTTONEAREST, MONITORINFO, MonitorFromPoint};
use windows::Win32::UI::WindowsAndMessaging::GetCursorPos;

use super::Overlay;
use super::geometry::SelRect;
use super::interaction::Phase;

/// 本次运行记住的上次选区（**屏幕坐标** `(x, y, w, h)`）。
/// 覆盖层每次截图都是一个新 `Overlay`，记忆只能放在模块级静态里；
/// 不写配置文件（ADR 0040：重启即失效，也就没有跨版本坐标漂移的问题）。
static LAST_AREA: Mutex<Option<(i32, i32, i32, i32)>> = Mutex::new(None);

/// 记住一块选区（屏幕坐标）。**只**由「手框松手 / 窗口吸附单击」两条路径调用。
pub(in crate::overlay) fn remember(x: i32, y: i32, w: i32, h: i32) {
    // 退化矩形没有复用价值（1×1 的误触），直接忽略
    if w <= 0 || h <= 0 {
        return;
    }
    if let Ok(mut slot) = LAST_AREA.lock() {
        *slot = Some((x, y, w, h));
    }
}

/// 取出上次选区（屏幕坐标）。无记忆或锁中毒时返回 `None`（静默）。
fn last_area() -> Option<(i32, i32, i32, i32)> {
    LAST_AREA.lock().ok().and_then(|slot| *slot)
}

/// 光标所在显示器的整屏矩形（屏幕坐标，含任务栏）。
fn monitor_rect_at_cursor() -> Option<(i32, i32, i32, i32)> {
    unsafe {
        let mut pt = POINT::default();
        if GetCursorPos(&mut pt).is_err() {
            return None;
        }
        let monitor = MonitorFromPoint(pt, MONITOR_DEFAULTTONEAREST);
        let mut info = MONITORINFO {
            cbSize: std::mem::size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        if !GetMonitorInfoW(monitor, &mut info).as_bool() {
            return None;
        }
        let r = info.rcMonitor;
        Some((r.left, r.top, r.right - r.left, r.bottom - r.top))
    }
}

/// 屏幕矩形 → 画布选区：与画布求交（显示器拔掉 / 分辨率改了自动裁小），
/// 完全落在画布外时返回 `None`（调用方静默不响应）。
fn clip_to_canvas(rect: (i32, i32, i32, i32), origin: (i32, i32), vw: i32, vh: i32) -> Option<SelRect> {
    let (x, y, w, h) = rect;
    let x0 = x.max(origin.0);
    let y0 = y.max(origin.1);
    let x1 = (x + w).min(origin.0 + vw);
    let y1 = (y + h).min(origin.1 + vh);
    if x1 <= x0 || y1 <= y0 {
        return None;
    }
    Some(SelRect {
        x: x0 - origin.0,
        y: y0 - origin.1,
        w: x1 - x0,
        h: y1 - y0,
    })
}

impl Overlay {
    /// 记住一块选区（画布坐标）作为「上次区域」：手框 / 移动 / 手柄缩放的松手
    /// 与窗口吸附单击两条路径调用。**F / R 自己产生的选区不调它**（见模块文档）。
    pub(super) fn remember_selection(&self, sel: SelRect) {
        let (sx, sy) = super::annotate::to_screen(sel, self.capture.origin_x, self.capture.origin_y);
        remember(sx, sy, sel.w, sel.h);
    }

    /// 全屏键（默认 F）：光标所在显示器的整屏作为选区。
    pub(super) fn on_select_fullscreen(&mut self) {
        if !self.quick_select_allowed() {
            return;
        }
        let Some(mon) = monitor_rect_at_cursor() else {
            return;
        };
        let sel = clip_to_canvas(mon, self.canvas_origin(), self.canvas_w(), self.canvas_h());
        if let Some(sel) = sel {
            self.apply_quick_selection(sel);
        }
    }

    /// 上次区域键（默认 R）：复用本次运行里上次框选 / 窗口吸附确定的区域。
    pub(super) fn on_select_last_area(&mut self) {
        if !self.quick_select_allowed() {
            return;
        }
        let Some(area) = last_area() else {
            return;
        };
        let sel = clip_to_canvas(area, self.canvas_origin(), self.canvas_w(), self.canvas_h());
        if let Some(sel) = sel {
            self.apply_quick_selection(sel);
        }
    }

    /// 生效条件：还没选区、且不是贴图编辑模式（选区锁定）。
    fn quick_select_allowed(&self) -> bool {
        !self.locked_selection && self.selection.is_none()
    }

    /// 画布原点（虚拟屏左上角，屏幕坐标）。
    fn canvas_origin(&self) -> (i32, i32) {
        (self.capture.origin_x, self.capture.origin_y)
    }

    fn canvas_w(&self) -> i32 {
        self.display.width() as i32
    }

    fn canvas_h(&self) -> i32 {
        self.display.height() as i32
    }

    /// 把程序算出的选区落到界面上：与窗口吸附单击同一条确认路径
    /// （清候选 → 进调整态 → 建选区 → 把窗口那半亮区重绘回暗图）。
    /// 不走「记住上次区域」那条路径（见模块文档）。
    fn apply_quick_selection(&mut self, sel: SelRect) {
        // 候选矩形随后的 `set_selection` 会覆盖重绘；窗口那一块（半亮区）不在选区脏区里，
        // 必须单独重绘，否则标题栏 / 状态栏会一直是半亮（ADR 0039 三档亮度）。
        let (candidate, window) = self.snap.clear();
        // 与窗口吸附单击一致：此后的双击就是「双击选区空白」的完成动作
        self.down_started_draw = true;
        self.set_selected(None);
        self.phase = Phase::Adjusting;
        self.set_selection(Some(sel));
        self.repaint_snap(candidate, None, [window, None], true);
        self.apply_cursor_now(self.hwnd);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sel(x: i32, y: i32, w: i32, h: i32) -> SelRect {
        SelRect { x, y, w, h }
    }

    /// 记忆只按屏幕坐标原样存，取回复用；退化矩形不入库。
    #[test]
    fn remembered_area_round_trips() {
        remember(-1200, 80, 800, 600);
        assert_eq!(last_area(), Some((-1200, 80, 800, 600)));
        remember(0, 0, 0, 600);
        assert_eq!(last_area(), Some((-1200, 80, 800, 600)), "零宽矩形不应覆盖记忆");
        remember(10, 10, 200, 100);
        assert_eq!(last_area(), Some((10, 10, 200, 100)), "后来的记忆应覆盖旧的");
    }

    /// 求交：完全在内 → 只平移到画布坐标；跨边界 → 裁小；完全在外 → None。
    #[test]
    fn clip_to_canvas_intersects_with_virtual_screen() {
        // 副屏在左侧：虚拟屏原点为负，画布 (0,0) 对应屏幕 (-1920, 0)
        let origin = (-1920, 0);
        let (vw, vh) = (3840, 1080);
        assert_eq!(
            clip_to_canvas((-1920, 0, 1920, 1080), origin, vw, vh),
            Some(sel(0, 0, 1920, 1080)),
            "整块副屏应原样换算"
        );
        assert_eq!(
            clip_to_canvas((1700, 100, 400, 400), origin, vw, vh),
            Some(sel(3620, 100, 220, 400)),
            "跨到虚拟屏右边界时应裁小（屏幕 x=1920 对应画布 x=3840）"
        );
        assert_eq!(
            clip_to_canvas((5000, 0, 100, 100), origin, vw, vh),
            None,
            "完全在虚拟屏之外（显示器已被拔掉）应无结果"
        );
        assert_eq!(
            clip_to_canvas((-4000, 0, 1920, 1080), origin, vw, vh),
            None,
            "整块屏都在虚拟屏左侧之外（交集为空）应无结果"
        );
    }
}
