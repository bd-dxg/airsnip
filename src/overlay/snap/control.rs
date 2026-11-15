// SPDX-License-Identifier: GPL-3.0-only
//! 控件级捕捉的覆盖层侧调度（CAP-6 二级 / ADR 0039）：从窗口候选下钻到 UIA 子控件、
//! 滚轮爬层级、以及候选框过渡的接线。
//!
//! 真正跑 UIA 的是 `uia` 的 worker 线程；这里只管「什么时候发请求」与「回包怎么用」，
//! 保证 UI 线程永不等待（目标无响应时 UIA 会阻塞 10 秒，见 `docs/uia-probe.md`）。

use windows::Win32::Foundation::POINT;
use windows::Win32::UI::WindowsAndMessaging::GetCursorPos;

use super::super::geometry::SelRect;
use super::anim;

impl super::super::Overlay {
    /// 发一次控件级查询：只在开关打开、窗口未熔断时。
    pub(super) fn request_controls(&mut self, hwnd: isize, sx: i32, sy: i32, frame: SelRect) {
        // 幂等：worker 已在 `run` 里起来时这句是空操作。传的必须是**覆盖层自己**的句柄
        // （worker 拿它 `PostMessage` 唤醒），**不是** `hwnd`（那是被吸附的目标窗口）。
        self.uia.spawn(self.hwnd);
        // worker 全程用**屏幕坐标**：窗口边框也得加上抓屏原点，否则副屏（负坐标）下裁错了
        let (ox, oy) = (self.capture.origin_x, self.capture.origin_y);
        let screen_frame = (frame.x + ox, frame.y + oy, frame.w, frame.h);
        let _ = self.uia.request(hwnd, sx, sy, screen_frame);
    }

    /// 控件级回包（`WM_SNAP_RESULT`）：仍是同一个窗口、且光标仍在目标控件上才用，否则丢弃。
    /// 回包里的坐标是**屏幕物理像素**（与 worker 同一口径），这里转回覆盖层坐标。
    pub(in crate::overlay) fn on_uia_reply(&mut self) {
        let Some((hwnd, levels)) = self.uia.take() else {
            return;
        };
        // 已经换窗口，或按下待定中（`pending.rect` 已定死）：此刻改候选会让
        // 「看到的」与「松手选中的」分叉，回包直接作废（ADR 0039 §12）。
        if hwnd != self.snap.hwnd() || self.snap.pending.is_some() {
            return;
        }
        let (ox, oy) = (self.capture.origin_x, self.capture.origin_y);
        let levels: Vec<SelRect> = levels
            .into_iter()
            .map(|(x, y, w, h)| SelRect {
                x: x - ox,
                y: y - oy,
                w,
                h,
            })
            .collect();
        // 层级是按**请求发出时**的光标点算的；回包抵达前光标可能已移到另一个控件。
        // 换成它就会高亮一块光标不在里面的矩形，按下去还会退化成自由框选
        // （`begin_snap_pending` 的 `contains` 判定失败），所以只接受「最深一级仍含当前光标」的回包。
        let Some(deepest) = levels.first() else {
            return;
        };
        let mut cur = POINT::default();
        if unsafe { GetCursorPos(&mut cur) }.is_err() {
            return;
        }
        if !deepest.contains(cur.x - ox, cur.y - oy) {
            return;
        }
        if let Some(target) = self.snap.set_levels(levels) {
            self.snap_to(target, true);
        }
    }

    /// 滚轮爬层级（ADR 0039）：在候选上滚动，"上"往父级、"下"往更深；到顶/到底就什么也不做。
    /// 返回是否消费了这次滚动。
    pub(in crate::overlay) fn climb_snap(&mut self, up: bool) -> bool {
        if self.snap.hwnd() == 0 || self.locked_selection || self.selection.is_some() {
            return false;
        }
        // 按下待定中：候选已经被 `pending.rect` 定死，再改会让松手选中另一块矩形（ADR 0039 §12）。
        if self.snap.pending.is_some() {
            return false;
        }
        // 只在移动态爬层级：绘制态（用户已选工具）的滚轮保留「调线宽 / 字号 / 模糊半径」，
        // 那是无选区时就存在的老交互，不能被候选吞掉（ADR 0039 §1）。
        if self.mode.tool().is_some() {
            return false;
        }
        let Some(target) = self.snap.climb(up) else {
            return false;
        };
        self.snap_to(target, true);
        true
    }

    /// 候选改成 `target`：任一边至少移动 6px 就做 90ms 过渡，否则直接跳。
    /// 过渡期间 `display` 里画的是插值矩形，每帧只重绘新旧两帧之间的带状区域。
    pub(super) fn snap_to(&mut self, target: SelRect, present: bool) {
        let prev = self.snap.candidate;
        // 换窗口（或窗口移动了）：旧窗口从半亮改回暗、新窗口从暗改成半亮，两块都得整块重绘。
        // 只在窗口真的变了时才带上它们——否则每次控件级回包 / 动画帧都得重绘整个窗口。
        let prev_window = self.snap.take_prev_window();
        let windows = if prev_window.is_some() {
            [prev_window, self.snap.window()]
        } else {
            [None, None]
        };
        if present && prev.is_some_and(|p| anim::worth_animating(p, target)) {
            self.snap.start_anim(target);
            anim::arm_timer(self.hwnd, true);
            // 动画起点的窗口变化也得先画掉（否则旧半亮区会一直留到下一帧）
            if windows.iter().any(Option::is_some) {
                self.repaint_snap(None, None, windows, present);
            }
            return;
        }
        self.snap.set_candidate(target);
        self.repaint_snap(prev, Some(target), windows, present);
    }

    /// 候选过渡定时器到点：推一帧并重绘带状区域；结束时收掉一次性定时器。
    pub(in crate::overlay) fn on_snap_anim_timer(&mut self) {
        match self.snap.anim_tick() {
            Some((prev, next, done)) => {
                if done {
                    anim::arm_timer(self.hwnd, false);
                }
                self.repaint_snap(Some(prev), Some(next), [None, None], true);
            }
            // 动画已被取消（清空 / 直接跳）：把定时器收掉
            None => anim::arm_timer(self.hwnd, false),
        }
    }
}
