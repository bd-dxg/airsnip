// SPDX-License-Identifier: GPL-3.0-only
//! 候选框过渡动画（CAP-6 二级 / ADR 0039 §12）：小控件 → 大控件时平滑长大，而不是硬跳。
//! 15ms × 6 ≈ 90ms（缓出），每帧只重绘「上一帧与这一帧之间」的带状区域，开销远小于整块重画。
//! 动画状态自包含（`Anim`），`Snap` 只负责把当前帧写进候选矩形并算脏区。

use windows::Win32::Foundation::HWND;
use windows::Win32::UI::WindowsAndMessaging::{KillTimer, SetTimer};

use super::super::geometry::SelRect;

/// 定时器周期与总帧数（15ms × 6 ≈ 90ms）。
const TICK_MS: u32 = 15;
const FRAMES: u32 = 6;
/// 任一边位移小于它就是「没变化」，不做动画（避免轻微跳动时拖沙）。
const MIN_STEP: i32 = 6;

/// 过渡定时器 id（覆盖层线程内：悬停提示 2 / 取消提示 3 / 淡入 4 / 候选过渡 5）。
pub(super) const TIMER_ID: usize = 5;

/// 一次候选过渡：从 `cur` 走到 `to`，`step` 为已推进的帧数。
#[derive(Clone, Copy)]
pub(super) struct Anim {
    from: SelRect,
    cur: SelRect,
    to: SelRect,
    step: u32,
}

impl Anim {
    /// 起一次过渡。`from` 必须是**当前显示**的矩形——动画中途换目标时若仍传原始起点，
    /// 画面上会看到一帧回跳。
    pub(super) fn new(from: SelRect, to: SelRect) -> Self {
        Self {
            from,
            cur: from,
            to,
            step: 0,
        }
    }

    /// 终点：按下取候选前必须落到它（否则会选中一个中间尺寸）。
    pub(super) fn target(&self) -> SelRect {
        self.to
    }

    /// 推一帧：返回 `(上一帧, 这一帧, 是否结束)`。
    pub(super) fn advance(&mut self) -> (SelRect, SelRect, bool) {
        let prev = self.cur;
        self.step += 1;
        let done = self.step >= FRAMES;
        let next = if done {
            self.to
        } else {
            lerp_rect(self.from, self.to, ease_out(self.step, FRAMES))
        };
        self.cur = next;
        (prev, next, done)
    }
}

/// 任一边位移是否达到「值得做过渡」的门槛。
pub(super) fn worth_animating(from: SelRect, to: SelRect) -> bool {
    rect_delta(from, to) >= MIN_STEP
}

/// 四边的最大位移。
fn rect_delta(a: SelRect, b: SelRect) -> i32 {
    [
        (a.x - b.x).abs(),
        (a.y - b.y).abs(),
        (a.w - b.w).abs(),
        (a.h - b.h).abs(),
    ]
    .into_iter()
    .max()
    .unwrap_or(0)
}

/// 缓出插值参数：`t ∈ (0,1)`，前快后慢。
fn ease_out(step: u32, total: u32) -> f32 {
    let t = step as f32 / total as f32;
    1.0 - (1.0 - t) * (1.0 - t)
}

/// 矩形插值（四边分别取整，避免累计误差）。
fn lerp_rect(a: SelRect, b: SelRect, t: f32) -> SelRect {
    let l = |x: i32, y: i32| (x as f32 + (y - x) as f32 * t).round() as i32;
    SelRect {
        x: l(a.x, b.x),
        y: l(a.y, b.y),
        w: l(a.w, b.w),
        h: l(a.h, b.h),
    }
}

/// 一次性定时器的起停。
pub(super) fn arm_timer(hwnd: HWND, on: bool) {
    unsafe {
        if on {
            // 同 ID 再次 SetTimer 会替换旧定时器（MSDN），无需先 Kill
            let _ = SetTimer(Some(hwnd), TIMER_ID, TICK_MS, None);
        } else {
            let _ = KillTimer(Some(hwnd), TIMER_ID);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 过渡（ADR 0039 §12）：逐帧插值、单调长大、最后一帧正好落在目标上。
    #[test]
    fn animation_walks_to_target() {
        let small = SelRect {
            x: 10,
            y: 10,
            w: 40,
            h: 20,
        };
        let big = SelRect {
            x: 100,
            y: 100,
            w: 400,
            h: 300,
        };
        assert!(worth_animating(small, big), "差这么多应该做过渡");
        assert!(!worth_animating(small, small), "没变化不做过渡");

        let mut anim = Anim::new(small, big);
        let mut last = small;
        let mut frames = 0;
        loop {
            let (prev, next, done) = anim.advance();
            assert_eq!(prev, last, "每帧都从上一次显示的位置继续");
            assert!(next.w >= last.w && next.h >= last.h, "应单调长大：{last:?} → {next:?}");
            last = next;
            frames += 1;
            if done {
                break;
            }
            assert!(frames < 32, "帧数必须有界");
        }
        assert_eq!(last, anim.target(), "结束时必须正好落在目标上");
        assert_eq!(frames, FRAMES as usize, "帧数固定（每帧一条定时器消息）");
    }

    /// 中途换目标：从当前显示位置接着走，不回跳（按下时 `finish_anim` 用到 `target`）。
    #[test]
    fn animation_restarts_from_current_frame() {
        let a = SelRect {
            x: 0,
            y: 0,
            w: 40,
            h: 20,
        };
        let b = SelRect {
            x: 0,
            y: 0,
            w: 200,
            h: 100,
        };
        let c = SelRect {
            x: 0,
            y: 0,
            w: 400,
            h: 200,
        };
        let mut anim = Anim::new(a, b);
        let (_, mid, done) = anim.advance();
        assert!(!done);
        let mut next_anim = Anim::new(mid, c);
        let (prev, ..) = next_anim.advance();
        assert_eq!(prev, mid, "新过渡应从当前显示位置开始，不能回跳到 a");
        assert_eq!(next_anim.target(), c);
    }
}
