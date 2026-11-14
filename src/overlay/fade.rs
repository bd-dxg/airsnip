// SPDX-License-Identifier: GPL-3.0-only
//! 覆盖层淡入（CAP-2 显示动画）：窗口显示后把**整窗常量 alpha** 从 0 推到 255，120ms / 8 帧。
//! 走 `WS_EX_LAYERED` + `SetLayeredWindowAttributes(LWA_ALPHA)`：每帧只有一次 API 调用、
//! 零像素成本（由 DWM 混合），不像 GDI 逐帧混合那样每帧要重贴一次全屏（4K 下每帧 ~25ms 会卡）。
//!
//! **动画结束立刻摘掉 `WS_EX_LAYERED`**，回到原来的纯 GDI 上屏路径——交互期间不常驻分层合成，
//! 「交互 ≥60 FPS」的性能红线不受影响。一次性定时器链，收尾即 `KillTimer`，不引入轮询（待机 0% CPU）。

use windows::Win32::Foundation::{COLORREF, LPARAM, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::{
    GWL_EXSTYLE, GetWindowLongPtrW, KillTimer, LWA_ALPHA, PostMessageW, SetLayeredWindowAttributes, SetTimer,
    SetWindowLongPtrW, WS_EX_LAYERED,
};

use super::geometry::SelRect;

/// 淡入定时器 ID（覆盖层线程内的资源 id 空间：悬停提示 2 / 取消提示 3 / 淡入 4）。
pub(super) const TIMER_ID: usize = 4;
/// 每帧间隔与帧数：15ms × 8 ≈ 120ms。
const TICK_MS: u32 = 15;
/// 总帧数。
pub(super) const STEPS: u32 = 8;

/// 第 `step` 帧（1..=STEPS）的窗口 alpha，缓出（先快后慢）：`255 × (1 − (1 − t)²)`。
/// 末帧正好 255，与 `finish_fade` 的收尾值一致，不会出现「最后一帧又跳回去」。
fn alpha_at(step: u32) -> u8 {
    let t = (step.min(STEPS) as f32) / STEPS as f32;
    let k = 1.0 - (1.0 - t) * (1.0 - t);
    (k * 255.0).round() as u8
}

impl super::Overlay {
    /// 显示**之前**调用：加分层样式并把 alpha 置 0，再启动一次性定时器链。
    /// 失败（`SetTimer` 返回 0）就直接收尾成不透明，宁可不淡入也不能让窗口一直是透明的。
    pub(super) fn begin_fade(&mut self) {
        unsafe {
            let ex = GetWindowLongPtrW(self.hwnd, GWL_EXSTYLE);
            SetWindowLongPtrW(self.hwnd, GWL_EXSTYLE, ex | WS_EX_LAYERED.0 as isize);
            let _ = SetLayeredWindowAttributes(self.hwnd, COLORREF(0), 0, LWA_ALPHA);
            if SetTimer(Some(self.hwnd), TIMER_ID, TICK_MS, None) == 0 {
                self.finish_fade();
            }
        }
    }

    /// 定时器到点：推一帧 alpha；最后一帧收尾。
    pub(super) fn on_fade_timer(&mut self) {
        if self.fade_step >= STEPS {
            return;
        }
        self.fade_step += 1;
        if self.fade_step >= STEPS {
            self.finish_fade();
            return;
        }
        unsafe {
            let _ = SetLayeredWindowAttributes(self.hwnd, COLORREF(0), alpha_at(self.fade_step), LWA_ALPHA);
        }
    }

    /// 收尾：alpha 拉满 → 摘掉分层样式 → 整窗重绘（幂等）。
    /// 摘样式后 DWM 需要一次完整的窗口内容，否则可能留着上一帧的半透明快照。
    fn finish_fade(&mut self) {
        if self.fade_step >= STEPS {
            return;
        }
        self.fade_step = STEPS;
        unsafe {
            let _ = KillTimer(Some(self.hwnd), TIMER_ID);
            let _ = SetLayeredWindowAttributes(self.hwnd, COLORREF(0), 255, LWA_ALPHA);
            let ex = GetWindowLongPtrW(self.hwnd, GWL_EXSTYLE);
            SetWindowLongPtrW(self.hwnd, GWL_EXSTYLE, ex & !(WS_EX_LAYERED.0 as isize));
        }
        let (w, h) = (self.display.width() as i32, self.display.height() as i32);
        self.repaint(SelRect { x: 0, y: 0, w, h });
        // 淡入期间是半透明窗口，前台/焦点可能没拿到手：再异步补一次（消息处理里执行，不阻塞）
        unsafe {
            let _ = PostMessageW(Some(self.hwnd), super::wndproc::WM_FOCUS_OVERLAY, WPARAM(0), LPARAM(0));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{STEPS, alpha_at};

    /// 缓出曲线：单调不减、首帧就已有存在感、末帧正好不透明。
    #[test]
    fn alpha_curve_is_monotonic_ease_out() {
        assert_eq!(alpha_at(0), 0, "第 0 帧应完全透明");
        let mut prev = 0;
        for step in 1..=STEPS {
            let a = alpha_at(step);
            assert!(a >= prev, "alpha 必须单调不减：step {step} → {a} < {prev}");
            prev = a;
        }
        assert_eq!(prev, 255, "末帧必须完全不透明，否则窗口会一直半透明");
        // 缓出：前半程已经走完一半以上，避免「前 60ms 像没出现」
        assert!(
            alpha_at(STEPS / 2) as f32 >= 255.0 * 0.6,
            "前半程 alpha 偏低：{}",
            alpha_at(STEPS / 2)
        );
        // 越界不炸，也不越过 255
        assert_eq!(alpha_at(STEPS + 5), 255);
    }
}
