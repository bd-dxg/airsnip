// SPDX-License-Identifier: GPL-3.0-only
//! 窗口吸附（CAP-6，issue #62，ADR 0037）：没有选区时悬停高亮光标下的顶层窗口矩形，单击即选中；
//! 按下后拖出 `CLICK_SLOP` 退回自由框选。候选只在没有选区时存在，与选中态互斥。
//! 命中查询见 `hit`（Z 序枚举 + DWM 扩展边框），候选框绘制见 `draw`，
//! 候选框过渡动画见 `anim`，控件级下钻（UIA）的调度见 `control`。

mod anim;
mod control;
mod draw;
mod hit;
mod levels;

pub(in crate::overlay) use draw::{draw_candidate, shade_candidate};

use windows::Win32::Foundation::POINT;
use windows::Win32::UI::WindowsAndMessaging::GetCursorPos;

use super::geometry::{SelRect, expand_rect, text_rect, union_rect};
use super::surface::DIM_K;
use super::ui::Ui;

/// 窗口候选的亮度（ADR 0037）：在原图基础上只压暗四分之一（暗化量取 `DIM_K` 的四分之一）。
/// 不回贴原图而是「半亮」，因为候选常常就是整屏（任务栏自动隐藏 + 窗口最大化、全屏窗口）——
/// 全亮的话暗化一点不剩，用户看不出覆盖层已经生效，只会以为 F1 没反应。
/// 但也不宜太暗（初版是暗化量减半 = 195 / 76%，实测窗口偏灰），取四分之一 = 225 / 88%。
///
/// 控件级下钻（ADR 0039）后这里只负责**窗口**这一档：控件矩形直接回贴原图（全亮），
/// 于是亮度分三档「桌面暗 < 窗口半亮 < 控件全亮」——窗口内部不再出现「一半亮一半暗」的断层。
pub(super) const CAND_K: u32 = DIM_K + (256 - DIM_K) * 3 / 4;

/// 亮度不变量（编译期就能判定，写错直接编译失败）：候选必须落在「暗图 < 候选 < 原图」之间。
/// 候选占满屏时，这个差值就是「F1 到底生效了没」的唯一信号。
const _: () = assert!(
    CAND_K > DIM_K && CAND_K < 256,
    "候选亮度必须在暗图与原图之间（ADR 0037）"
);

/// 窗口档的取值口径（ADR 0037 / ADR 0039 §14）：约 88%（225）。
/// 太低（初版的 195 / 76%）实测窗口偏灰；太高就与控件全亮（255）挤在一起，两层分不出来。
const _: () = assert!(
    CAND_K >= 215 && CAND_K <= 240,
    "窗口档应取约 88%（225），别偏回 76% 也别贴近全亮"
);

/// 按下时的「候选待定」：按下点 + 候选矩形（拖出阈值就作废，见 `interaction::mouse`）。
#[derive(Clone, Copy)]
pub(super) struct Pending {
    pub(super) x: i32,
    pub(super) y: i32,
    pub(super) rect: SelRect,
}

/// 候选的矩形还在，不重查的光标移动上限（像素）：在「大元素」（比如整个文档）里
/// 光标小范围移动时不必重查，超过它才重新下钻。
const REQUERY_STEP: i32 = 24;
/// 上一次查询什么都没拿到（该程序不暴露子元素，比如自绘界面 / UWP 宿主）时，
/// 要用大得多的步长才重试，否则鼠标每挪一点就白查一次。
const REQUERY_STEP_EMPTY: i32 = 120;

/// 候选框过渡定时器 id（实现在 `anim`，这里转发一份供 `wndproc` 与 `recapture` 用）。
pub(super) const ANIM_TIMER_ID: usize = anim::TIMER_ID;

/// 吸附状态（CAP-6 / ADR 0037 / ADR 0039）。
#[derive(Default)]
pub(super) struct Snap {
    /// 当前要高亮的矩形（覆盖层坐标）：整窗或已识别出的控件。
    pub(super) candidate: Option<SelRect>,
    /// 候选对应的窗口句柄（原生值，0 = 无候选）。
    hwnd: isize,
    /// 当前窗口的整窗矩形（候选的兜底层级，控件级回包未到时就是它）。
    window: Option<SelRect>,
    /// 上一帧画过的窗口矩形：换窗口 / 窗口移动时它要从半亮改回暗，整块得重绘。
    prev_window: Option<SelRect>,
    /// 控件层级（由深到浅，已裁到窗口内）；空 = 没有更细的信息。
    levels: levels::Levels,
    /// 上次发起控件级查询时的光标位置（覆盖层坐标）。
    last_query: Option<(i32, i32)>,
    /// 正在做的大小过渡（`None` = 静止）。
    anim: Option<anim::Anim>,
    /// 按下但还没决定「选整窗」还是「自由框选」。
    pub(super) pending: Option<Pending>,
}

impl Snap {
    /// 当前候选窗口句柄。
    pub(super) fn hwnd(&self) -> isize {
        self.hwnd
    }

    /// 当前窗口的整窗矩形。
    pub(super) fn window(&self) -> Option<SelRect> {
        self.window
    }

    /// 换上新的候选窗口（hwnd 或整窗矩形变了才调用）：先给整窗，控件级回包到了再细化。
    /// 旧窗口记在 `prev_window` 里——它的半亮区域要整块改回暗（见 `snap_to`）。
    pub(super) fn set_window(&mut self, hwnd: isize, rect: SelRect) {
        self.hwnd = hwnd;
        self.prev_window = self.window;
        self.window = Some(rect);
        self.levels.clear();
        self.last_query = None;
        self.candidate = Some(rect);
    }

    /// 取走「上一帧的窗口」：只有换过窗口（或窗口移动过）时才 `Some`。
    pub(super) fn take_prev_window(&mut self) -> Option<SelRect> {
        self.prev_window.take()
    }

    /// 当前候选是否已下钻到控件层（比整窗更细）：决定渲染时要不要回贴原图成全亮。
    /// 整窗一档（含滚轮爬回整窗）必须保持半亮——最大化窗口 / 全屏窗口占满屏时要靠它
    /// 看出覆盖层已经生效（ADR 0037），所以不能简单地「总是回贴原图」。
    pub(super) fn is_finer_than_window(&self) -> bool {
        matches!((self.candidate, self.window), (Some(c), Some(w)) if c != w)
    }

    /// 这次鼠标位置是否需要重新查控件级（ADR 0039）：
    /// - 光标还在当前候选内部、且没走远 → 不必重查（避免每次鼠标移动发一次请求）；
    /// - 光标离开候选（跨到另一个控件）→ 立即重查；
    /// - 光标在候选内但已走 `REQUERY_STEP` 以上 → 也重查（否则在大元素里永远细化不下去）。
    pub(super) fn needs_query(&self, x: i32, y: i32) -> bool {
        let step = if self.levels.is_empty() {
            REQUERY_STEP_EMPTY
        } else {
            REQUERY_STEP
        };
        // 过渡中拿**终点**判「还在不在里面」：插值矩形还没长到终点时，光标其实已经在目标控件上，
        // 用中间帧会误判成「离开候选」而每帧重发一次查询。
        let shown = self.anim.map_or(self.candidate, |a| Some(a.target()));
        let inside = shown.is_some_and(|c| c.contains(x, y));
        let far = self
            .last_query
            .is_none_or(|(lx, ly)| (x - lx).abs() >= step || (y - ly).abs() >= step);
        !inside || far
    }

    /// 记下这次查询的光标位置。
    pub(super) fn mark_queried(&mut self, x: i32, y: i32) {
        self.last_query = Some((x, y));
    }

    /// 直接换显示矩形（不做动画）。
    pub(super) fn set_candidate(&mut self, rect: SelRect) {
        self.candidate = Some(rect);
        self.anim = None;
    }

    /// 开始向 `target` 过渡：`candidate` 暂时保持不动，由定时器逐帧推。
    /// 动画中途又来了新目标时，从**当前显示位置**重新开始，看起来是连续的。
    pub(super) fn start_anim(&mut self, target: SelRect) {
        let from = self.candidate.unwrap_or(target);
        self.anim = Some(anim::Anim::new(from, target));
    }

    /// 推一帧：返回 `(上一帧显示, 这一帧显示, 是否结束)`；没在过渡时返回 `None`。
    pub(super) fn anim_tick(&mut self) -> Option<(SelRect, SelRect, bool)> {
        let anim = self.anim.as_mut()?;
        let (prev, next, done) = anim.advance();
        self.candidate = Some(next);
        if done {
            self.anim = None;
        }
        Some((prev, next, done))
    }

    /// 立即结束过渡（按下取候选前必须调，否则会选中一个中间矩形）。
    pub(super) fn finish_anim(&mut self) {
        if let Some(anim) = self.anim.take() {
            self.candidate = Some(anim.target());
        }
    }

    /// 回包到了：记下层级，返回应该显示的目标（最深一级）；没变化返回 `None`。
    pub(super) fn set_levels(&mut self, levels: Vec<SelRect>) -> Option<SelRect> {
        let target = self.levels.set(levels)?;
        (self.candidate != Some(target)).then_some(target)
    }

    /// 滚轮爬层级：`up` = 往父级（更粗）、否则往更深。返回新目标（已是最后一层时返回 `None`）。
    pub(super) fn climb(&mut self, up: bool) -> Option<SelRect> {
        self.levels.climb(up, self.window)
    }

    /// 清空候选，返回（旧候选, 旧窗口）：窗口那部份的半亮区要整块改回暗。
    pub(super) fn clear(&mut self) -> (Option<SelRect>, Option<SelRect>) {
        self.hwnd = 0;
        self.prev_window = None;
        self.levels.clear();
        self.last_query = None;
        self.anim = None;
        (self.candidate.take(), self.window.take())
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
                // 同一个窗口原地移动：矩形也必然相同，不重绘
                if self.snap.hwnd() != hwnd || self.snap.window() != Some(rect) {
                    self.snap.set_window(hwnd, rect);
                    self.snap_to(rect, present);
                }
                // 控件级：跟光标走。一个在途请求就够（回包后 `pending` 清掉才会再发），
                // 光标没离开当前控件时不重查——这样从 A 控件移到 B 控件会立即重查，
                // 而在同一个控件内部移动不会刷请求。
                if self.control_snap && present && !self.uia.busy() && self.snap.needs_query(x, y) {
                    self.snap.mark_queried(x, y);
                    self.request_controls(hwnd, x + ox, y + oy, rect);
                }
            }
            None if present => self.drop_snap(),
            None => {}
        }
    }

    /// 清空候选（连同旧候选与窗口半亮区的重绘）。
    pub(in crate::overlay) fn drop_snap(&mut self) {
        let (old, window) = self.snap.clear();
        self.repaint_snap(old, None, [window, None], true);
    }

    /// 重绘脏区：`old` / `new` 是候选矩形（各自含边框与尺寸提示），
    /// `windows` 是「换窗口」时要整块重绘的窗口矩形（半亮 ↔ 暗，没有任何装饰）。
    pub(super) fn repaint_snap(
        &mut self,
        old: Option<SelRect>,
        new: Option<SelRect>,
        windows: [Option<SelRect>; 2],
        present: bool,
    ) {
        let (vw, vh) = (self.display.width() as i32, self.display.height() as i32);
        let mut dirty = None;
        for r in [old, new].into_iter().flatten() {
            let a = expand_rect(r, 2);
            let b = candidate_text_rect(r, vw, vh, self.ui);
            dirty = union_rect(union_rect(dirty, Some(a)), Some(b));
        }
        // 窗口没有尺寸提示，外扩 2px 只为了盖住候选边框可能的溢出
        for r in windows.into_iter().flatten() {
            dirty = union_rect(dirty, Some(expand_rect(r, 2)));
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

    /// 控件级查询的触发节奏（ADR 0039 回归）：
    /// ① 首帧预画只设窗口候选、不发请求，所以入窗口后必须补一次；
    /// ② 同一窗口内**跨到另一个控件要重查**（旧实现「每窗口只查一次」导致切换控件很僵）；
    /// ③ 光标在当前控件内部小范围移动不重查，否则每次鼠标移动都发请求。
    #[test]
    fn control_query_follows_the_cursor() {
        let mut s = Snap::default();
        let window = SelRect {
            x: 0,
            y: 0,
            w: 400,
            h: 300,
        };
        s.set_window(7, window);
        assert!(s.needs_query(10, 10), "刚换上窗口应补查一次");

        // 回包后候选收紧到一个小控件
        let control = SelRect {
            x: 8,
            y: 8,
            w: 40,
            h: 20,
        };
        let target = s.set_levels(vec![control]).expect("应给出新目标");
        s.set_candidate(target);
        s.mark_queried(20, 15);

        // 在控件内部小范围移动：不重查
        assert!(!s.needs_query(22, 16), "控件内部小移动不应刷请求");
        // 离开控件（跨到另一个控件）：立即重查
        assert!(s.needs_query(200, 150), "跨到别的控件必须重查");
        // 在控件内但走远了（大元素里要能继续细化）
        s.mark_queried(10, 10);
        assert!(s.needs_query(40, 10), "走远应重查");

        // 什么都没有的窗口（levels 空）：小范围移动不该白查
        let mut e = Snap::default();
        e.set_window(9, window);
        assert!(e.needs_query(10, 10), "刚换上窗口应补查一次");
        e.mark_queried(10, 10);
        assert!(!e.needs_query(40, 10), "0 层的窗口小移动不应重查");
        assert!(e.needs_query(200, 10), "走很远才再试一次");

        // 换窗口 → 重新可查
        s.set_window(8, window);
        assert!(s.needs_query(10, 10));
    }

    /// 三档亮度（ADR 0039）：只有「控件比整窗细」才回贴原图成全亮；整窗一档必须留在半亮。
    /// 写反则两种退化：全亮 → 最大化窗口下看不出覆盖层生效；不回贴 → 控件与窗口没有视觉层次。
    #[test]
    fn only_control_level_is_finer_than_window() {
        let win = SelRect {
            x: 0,
            y: 0,
            w: 400,
            h: 300,
        };
        let control = SelRect {
            x: 8,
            y: 8,
            w: 40,
            h: 20,
        };
        let mut s = Snap::default();
        s.set_window(7, win);
        assert!(!s.is_finer_than_window(), "刚换上窗口只有整窗一档（半亮）");

        s.set_levels(vec![control]);
        s.set_candidate(control);
        assert!(s.is_finer_than_window(), "下钻到控件后应回贴原图成全亮");

        // 滚轮爬到整窗：回到半亮档
        assert_eq!(s.climb(true), Some(win));
        s.set_candidate(win);
        assert!(!s.is_finer_than_window(), "爬回整窗必须回到半亮");
    }

    /// 换窗口 / 清空候选时要把旧窗口交出来整块重绘：否则它的半亮区会留在暗图上（ADR 0039）。
    #[test]
    fn window_changes_hand_back_old_window() {
        let a = SelRect {
            x: 0,
            y: 0,
            w: 400,
            h: 300,
        };
        let b = SelRect {
            x: 100,
            y: 100,
            w: 600,
            h: 400,
        };
        let mut s = Snap::default();
        s.set_window(1, a);
        assert_eq!(s.take_prev_window(), None, "首次没有旧窗口要重绘");

        s.set_window(2, b);
        assert_eq!(s.take_prev_window(), Some(a), "换窗口要把旧窗口整块改回暗");
        assert_eq!(s.take_prev_window(), None, "取过一次就清掉，不重复重绘");

        let (cand, win) = s.clear();
        assert_eq!(cand, Some(b), "清空时返回旧候选");
        assert_eq!(win, Some(b), "清空时返回旧窗口（半亮区要回暗）");
        assert_eq!(s.take_prev_window(), None, "`clear` 后不应再留下旧窗口");
    }

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
