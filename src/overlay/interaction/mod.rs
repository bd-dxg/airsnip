// SPDX-License-Identifier: GPL-3.0-only
//! 鼠标与绘制交互（CAP-2、EDT-1/2/3/7、ADR 0036）：框选、八向缩放、平移选区（空格 / 移动态直接拖）、
//! 选区内绘制标注、选中对象的拖动与缩放。命中优先级由 `pick::pick_target` 统一给出（ADR 0023）：
//! 对象手柄 > 曲线控制柄 > 选区手柄 > 空格移动选区 > 移动态（对象 / 选区平移）
//! > Ctrl + 对象本体 > 选中对象包围框边缘 > 对象内部（绘制，薄位移松手则选中）> 空白处绘制。
//! 对象手柄与几何变换见 `objedit`；命中判定见 `pick`，绘制草稿见 `draft`，鼠标按下/移动/松开接线见 `mouse`。

mod draft;
mod mouse;

use super::geometry::SelRect;

/// 「拖了下」的判据（像素）：对象内部按下后位移超过它就转成绘制（ADR 0023 §3）。
/// 与 `pick::is_degenerate` 的 2px 呼应——小于它的草稿本来就会被丢弃。
pub(super) const CLICK_SLOP: i32 = 2;

/// 交互阶段。
#[derive(Clone, Copy, PartialEq)]
pub(super) enum Phase {
    /// 拖框选择中
    Selecting,
    /// 选区确认后，可移动/缩放/输出
    Adjusting,
}

/// 拖拽中的上下文（选区拖动；对象拖动见 `objedit::ObjDrag`）。
#[derive(Clone, Copy)]
pub(super) struct Drag {
    pub(super) handle: super::pick::Handle,
    /// 按下时的选区（用于计算增量）。
    orig: SelRect,
    /// 按下时的鼠标位置。
    px: i32,
    py: i32,
}

/// 「按下但还没决定干什么」：对象内部的按下先落到这里（ADR 0023 §3）。
/// 拖出 `CLICK_SLOP` 才转成绘制；直接松手则只选中 `index`。空白处的按下不走这里（那里立即绘制）。
#[derive(Clone, Copy)]
pub(super) struct PendingClick {
    pub(super) x: i32,
    pub(super) y: i32,
    /// 按下处命中的对象：无位移松手时选中它。
    pub(super) index: usize,
}

impl super::Overlay {
    /// 方向键微调（EDT-7 / CAP-2）：有选中对象 → 移动该对象（限制在选区内）；
    /// 否则 → 移动选区框（标注不跟随，允许落到选区外，导出以选区为准）。
    /// 每次按键 = 一步撤销。
    pub(super) fn nudge(&mut self, dx: i32, dy: i32) {
        // 拖拽 / 绘制 / 单击待定中忽略：这些流程每帧从按下时的几何重算，微调会被覆盖
        if self.obj_drag.is_some()
            || self.ctrl_drag.is_some()
            || self.drag.is_some()
            || self.draft.is_some()
            || self.pending_click.is_some()
        {
            return;
        }
        if self.selected.is_some() {
            self.nudge_object(dx, dy);
            return;
        }
        // 贴图编辑模式（ADR 0028）：选区锁定，方向键不得移动选区（否则写回会按偏移矩形裁剪）
        if self.locked_selection {
            return;
        }
        self.nudge_selection(dx, dy);
    }

    /// 微调选区框位置（CAP-2）：限制在虚拟屏幕内，工具栏随之重排。
    fn nudge_selection(&mut self, dx: i32, dy: i32) {
        let Some(sel) = self.selection else {
            return;
        };
        let (vw, vh) = (self.display.width() as i32, self.display.height() as i32);
        let x = (sel.x + dx).clamp(0, (vw - sel.w).max(0));
        let y = (sel.y + dy).clamp(0, (vh - sel.h).max(0));
        if x == sel.x && y == sel.y {
            return;
        }
        self.set_selection(Some(SelRect {
            x,
            y,
            w: sel.w,
            h: sel.h,
        }));
    }
}
