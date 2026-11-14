// SPDX-License-Identifier: GPL-3.0-only
//! 窗口吸附（CAP-6，issue #62，ADR 0037）：没有选区时悬停高亮光标下的顶层窗口矩形，单击即选中；
//! 按下后拖出 `CLICK_SLOP` 退回自由框选。候选只在没有选区时存在，与选中态互斥。
//! 命中查询见 `hit`（Z 序枚举 + DWM 扩展边框），候选框绘制见 `draw`。

mod draw;
mod hit;

pub(in crate::overlay) use draw::draw_candidate;

use windows::Win32::Foundation::POINT;
use windows::Win32::UI::WindowsAndMessaging::GetCursorPos;

use super::geometry::{SelRect, expand_rect, text_rect, union_rect};
use super::surface::DIM_K;
use super::ui::Ui;

/// 候选区亮度（ADR 0037）：在原图基础上压暗一档（暗化量减半）。
/// 不回贴原图而是「半亮」，因为候选常常就是整屏（任务栏自动隐藏 + 窗口最大化、全屏窗口）——
/// 全亮的话暗化一点不剩，用户看不出覆盖层已经生效，只会以为 F1 没反应。
pub(super) const CAND_K: u32 = DIM_K + (256 - DIM_K) / 2;

/// 亮度不变量（编译期就能判定，写错直接编译失败）：候选必须落在「暗图 < 候选 < 原图」之间。
/// 候选占满屏时，这个差值就是「F1 到底生效了没」的唯一信号。
const _: () = assert!(
    CAND_K > DIM_K && CAND_K < 256,
    "候选亮度必须在暗图与原图之间（ADR 0037）"
);

/// 按下时的「候选待定」：按下点 + 候选矩形（拖出阈值就作废，见 `interaction::mouse`）。
#[derive(Clone, Copy)]
pub(super) struct Pending {
    pub(super) x: i32,
    pub(super) y: i32,
    pub(super) rect: SelRect,
}

/// 吸附状态（CAP-6 / ADR 0037）。
#[derive(Default)]
pub(super) struct Snap {
    /// 光标下的候选窗口矩形（覆盖层坐标）。
    pub(super) candidate: Option<SelRect>,
    /// 候选对应的窗口句柄（原生值，0 = 无候选）：与矩形一起判「有没有变」。
    hwnd: isize,
    /// 按下但还没决定「选整窗」还是「自由框选」。
    pub(super) pending: Option<Pending>,
}

impl Snap {
    /// 当前候选窗口句柄。
    pub(super) fn hwnd(&self) -> isize {
        self.hwnd
    }

    /// 换候选，返回旧矩形（供调用方算脏区）。
    pub(super) fn set(&mut self, hwnd: isize, rect: Option<SelRect>) -> Option<SelRect> {
        self.hwnd = hwnd;
        std::mem::replace(&mut self.candidate, rect)
    }

    /// 清空候选，返回旧矩形。
    pub(super) fn clear(&mut self) -> Option<SelRect> {
        self.set(0, None)
    }
}

/// 候选的尺寸提示矩形：与选区用同一套避让（`text_rect`），再夹回屏幕内。
/// 最大化窗口的候选贴着屏幕底边，不夹的话提示会画到屏幕外，等于没画。
pub(super) fn candidate_text_rect(rect: SelRect, vw: i32, vh: i32, ui: Ui) -> SelRect {
    let mut r = text_rect(rect, vw, vh, 0, ui);
    // 上下各留 2px；屏太小放不下时宁可贴顶（不能给 `clamp` 传反了的上下界）
    let lo = 2;
    let hi = (vh - r.h - 2).max(lo);
    r.y = r.y.clamp(lo, hi);
    r
}

impl super::Overlay {
    /// 吸附是否生效：开关打开、没有选区（贴图编辑模式的选区从第一帧就锁定）、没在拖框。
    fn snap_enabled(&self) -> bool {
        self.window_snap && !self.locked_selection && self.selection.is_none() && self.drag.is_none()
    }

    /// 鼠标移动时更新候选窗口（CAP-6 / ADR 0037）。
    /// 只有候选真的变了才重绘——每次移动都重绘会让 4K 屏掉帧；也不做时间节流，
    /// 免得出现「移过去要等一下才亮」的迟滞。
    pub(in crate::overlay) fn update_snap(&mut self, x: i32, y: i32) {
        self.apply_snap(x, y, true);
    }

    /// 覆盖层**显示前**先把光标下的候选画进 `display`（不上屏，ADR 0037）。
    /// 这样 `UpdateWindow` 的首帧就是「暗化 + 半亮候选」，不会先闪一帧全暗。
    pub(super) fn prime_snap_at_cursor(&mut self) {
        let mut p = POINT::default();
        if unsafe { GetCursorPos(&mut p) }.is_err() {
            return;
        }
        let (ox, oy) = (self.capture.origin_x, self.capture.origin_y);
        self.apply_snap(p.x - ox, p.y - oy, false);
    }

    /// `present` 为假时只改 `display`、不上屏（供显示前的首帧准备用）。
    fn apply_snap(&mut self, x: i32, y: i32, present: bool) {
        if !self.snap_enabled() {
            if present {
                self.drop_snap();
            }
            return;
        }
        // 候选待定（按下了但还没决定）：保持原候选，阈值判定在 `interaction::mouse`。
        // 若左键已经松开（异常丢了 `WM_LBUTTONUP`，如 Alt+Tab 掉焦点），就丢掉待定，
        // 否则候选会死在那里、后续再也不会更新。
        if self.snap.pending.is_some() {
            if super::wndproc::lbutton_down() {
                return;
            }
            self.snap.pending = None;
        }
        let (ox, oy) = (self.capture.origin_x, self.capture.origin_y);
        let size = (self.display.width() as i32, self.display.height() as i32);
        match hit::window_at(self.hwnd, x + ox, y + oy, (ox, oy), size) {
            Some((hwnd, rect)) => {
                // 同一个窗口原地移动：矩形必然相同，不必重绘
                if self.snap.hwnd() == hwnd && self.snap.candidate == Some(rect) {
                    return;
                }
                let old = self.snap.set(hwnd, Some(rect));
                self.repaint_snap(old, Some(rect), present);
            }
            None if present => self.drop_snap(),
            None => {}
        }
    }

    /// 清空候选（连同旧候选的脏区重绘）。
    pub(in crate::overlay) fn drop_snap(&mut self) {
        if let Some(old) = self.snap.clear() {
            self.repaint_snap(Some(old), None, true);
        }
    }

    /// 候选变化后重绘「旧 + 新」两块脏区（各自含边框与尺寸提示）。
    fn repaint_snap(&mut self, old: Option<SelRect>, new: Option<SelRect>, present: bool) {
        let (vw, vh) = (self.display.width() as i32, self.display.height() as i32);
        let mut dirty = None;
        for r in [old, new].into_iter().flatten() {
            let a = expand_rect(r, 2);
            let b = candidate_text_rect(r, vw, vh, self.ui);
            dirty = union_rect(union_rect(dirty, Some(a)), Some(b));
        }
        if let Some(d) = dirty {
            if present {
                self.repaint(d);
            } else {
                self.draw_region(d);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 候选尺寸提示（ADR 0037）：贴底的整屏候选不能把提示画到屏幕外。
    #[test]
    fn candidate_hint_stays_on_screen() {
        let ui = Ui::from_scale(1.0);
        let (vw, vh) = (1920, 1080);

        // 整屏候选：`text_rect` 会把它放到 y = 屏高 + 8，必须夹回屏内
        let full = SelRect {
            x: 0,
            y: 0,
            w: vw,
            h: vh,
        };
        let r = candidate_text_rect(full, vw, vh, ui);
        assert!(r.y >= 0 && r.y + r.h <= vh, "提示应留在屏内：{r:?}");
        assert_eq!(r.x, text_rect(full, vw, vh, 0, ui).x, "横向沿用 `text_rect` 的贴边规则");

        // 屏幕中部的普通窗口：与选区同一套避让（上方放得下就放上方，ADR 0036 后一直是这个规则）
        let win = SelRect {
            x: 200,
            y: 200,
            w: 600,
            h: 400,
        };
        let r = candidate_text_rect(win, vw, vh, ui);
        assert_eq!(r, text_rect(win, vw, vh, 0, ui), "放得下时不应改动位置");
        assert!(r.y + r.h <= win.y, "上方放得下时提示应在窗口上方：{r:?}");
    }
}
